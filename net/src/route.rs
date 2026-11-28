//! Kernel routes told apart by their protocol tag, of either family, through rtnetlink, and the
//! uplink a family's default route leaves through.

use crate::{Family, IpNet};
use futures::{Stream, StreamExt, TryStreamExt};
use rtnetlink::packet_route::address::AddressMessage;
use rtnetlink::packet_route::route::{
    RouteAddress, RouteAttribute, RouteHeader, RouteMessage, RouteProtocol, RouteScope, RouteVia,
};
use rtnetlink::{Handle, RouteMessageBuilder};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::sync::Mutex;

pub const MAIN_TABLE: u32 = RouteHeader::RT_TABLE_MAIN as u32;

const ESRCH: i32 = -3;

/// A route as this module installs and lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub destination: IpNet,
    pub table: u32,
    pub protocol: u8,
    pub gateway: Option<IpAddr>,
    pub oif: Option<u32>,
    /// `None` is the kernel's default, which differs by family: 0 for IPv4, which wins over every
    /// other metric, and 1024 for IPv6, which loses to most. Listed, IPv4's 0 reads as `None`.
    pub metric: Option<u32>,
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} table {} proto {}",
            self.destination, self.table, self.protocol
        )?;
        if let Some(gateway) = self.gateway {
            write!(f, " via {gateway}")?;
        }
        if let Some(oif) = self.oif {
            write!(f, " dev {oif}")?;
        }
        if let Some(metric) = self.metric {
            write!(f, " metric {metric}")?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to open an rtnetlink socket")]
    Connect(#[source] std::io::Error),
    #[error("failed to subscribe to rtnetlink link, route and address changes")]
    Subscribe(#[source] std::io::Error),
    /// rtnetlink does not export the type of its refusal, only its text.
    #[error("cannot {action} route {route}: {reason}")]
    Invalid {
        action: &'static str,
        route: String,
        reason: String,
    },
    #[error("failed to {action}")]
    Request {
        action: String,
        #[source]
        source: rtnetlink::Error,
    },
}

fn request(action: impl FnOnce() -> String) -> impl FnOnce(rtnetlink::Error) -> Error {
    move |source| Error::Request {
        action: action(),
        source,
    }
}

#[derive(Clone)]
pub struct RtClient {
    handle: Handle,
    /// A shared netlink socket under concurrent requests can answer `EBUSY`, even across unrelated
    /// requests; one request at a time needs no retry budget. Not reentrant: a locked method calls
    /// only unlocked helpers.
    lock: Arc<Mutex<()>>,
}

impl RtClient {
    /// Spawns the socket's connection on the current tokio runtime.
    pub fn connect() -> Result<Self, Error> {
        let (connection, handle, _) = rtnetlink::new_connection().map_err(Error::Connect)?;
        tokio::spawn(connection);
        Ok(Self {
            handle,
            lock: Arc::new(Mutex::new(())),
        })
    }

    /// `ip route replace`: any route of the same destination, table, tos and metric is replaced,
    /// whoever installed it. The kernel does not match the protocol here.
    pub async fn route_replace(&self, route: &Route) -> Result<(), Error> {
        let _guard = self.lock.lock().await;
        self.handle
            .route()
            .add(route_message(route, "replace")?)
            .replace()
            .execute()
            .await
            .map_err(request(|| format!("replace route {route}")))
    }

    /// `ip route del`, scoped to the route's protocol: another daemon's route at the destination
    /// stays. A route already gone is deleted.
    pub async fn route_del(&self, route: &Route) -> Result<(), Error> {
        let _guard = self.lock.lock().await;
        let mut message = route_message(route, "delete")?;
        // The kernel matches an IPv4 route's scope on delete unless it is `RT_SCOPE_NOWHERE`.
        message.header.scope = RouteScope::NoWhere;
        match self.handle.route().del(message).execute().await {
            Err(rtnetlink::Error::NetlinkError(ref e)) if e.raw_code() == ESRCH => Ok(()),
            result => result.map_err(request(|| format!("delete route {route}"))),
        }
    }

    /// Every route of either family in `table` tagged `protocol`, in the kernel's order.
    pub async fn routes(&self, table: u32, protocol: u8) -> Result<Vec<Route>, Error> {
        let _guard = self.lock.lock().await;
        let mut out = Vec::new();
        for family in [Family::Ipv4, Family::Ipv6] {
            out.extend(
                self.dump(family)
                    .await?
                    .into_iter()
                    .filter_map(|message| route_of(&message))
                    .filter(|route| route.table == table && route.protocol == protocol),
            );
        }
        Ok(out)
    }

    /// The first `family` default route of the main table the kernel lists that leaves through a
    /// link. `None` is common on a single-stack node.
    pub async fn default_route(&self, family: Family) -> Result<Option<Route>, Error> {
        let _guard = self.lock.lock().await;
        self.default_route_unlocked(family).await
    }

    /// The link of the `family` default route, with its addresses of that family a packet can
    /// leave from, in the kernel's order. A dual-stack node's two need not share a link.
    pub async fn uplink(&self, family: Family) -> Result<Option<(String, Vec<IpAddr>)>, Error> {
        use rtnetlink::packet_route::AddressFamily;
        use rtnetlink::packet_route::link::{LinkAttribute, LinkMessage};

        let _guard = self.lock.lock().await;
        let Some(oif) = self
            .default_route_unlocked(family)
            .await?
            .and_then(|route| route.oif)
        else {
            return Ok(None);
        };
        let links: Vec<LinkMessage> = self
            .handle
            .link()
            .get()
            .execute()
            .try_collect()
            .await
            .map_err(request(|| "list the links".to_owned()))?;
        // Listed, not looked up by index: a link gone since the route was read is no uplink.
        let Some(name) = links
            .into_iter()
            .filter(|link| link.header.index == oif)
            .flat_map(|link| link.attributes)
            .find_map(|attribute| match attribute {
                LinkAttribute::IfName(name) => Some(name),
                _ => None,
            })
        else {
            return Ok(None);
        };
        let address_family = match family {
            Family::Ipv4 => AddressFamily::Inet,
            Family::Ipv6 => AddressFamily::Inet6,
        };
        let addresses: Vec<AddressMessage> = self
            .handle
            .address()
            .get()
            .set_link_index_filter(oif)
            .execute()
            .try_collect()
            .await
            .map_err(request(|| format!("list the addresses of link {name:?}")))?;
        let addresses = addresses
            .iter()
            .filter(|message| message.header.family == address_family)
            .filter_map(source_address)
            .collect();
        Ok(Some((name, addresses)))
    }

    async fn default_route_unlocked(&self, family: Family) -> Result<Option<Route>, Error> {
        Ok(self
            .dump(family)
            .await?
            .iter()
            .filter(|message| message.header.destination_prefix_length == 0)
            .filter_map(route_of)
            .find(|route| route.table == MAIN_TABLE && route.oif.is_some()))
    }

    async fn dump(&self, family: Family) -> Result<Vec<RouteMessage>, Error> {
        let query = match family {
            Family::Ipv4 => RouteMessageBuilder::<Ipv4Addr>::new().build(),
            Family::Ipv6 => RouteMessageBuilder::<Ipv6Addr>::new().build(),
        };
        self.handle
            .route()
            .get(query)
            .execute()
            .try_collect()
            .await
            .map_err(request(|| format!("list the {family:?} routes")))
    }
}

/// A wake-up for every change to a link, or to a route or an address of either family, an overrun
/// of the socket included. Nothing is taken from the messages: whoever wakes looks up the state
/// again. Links too, since the kernel drops the IPv4 routes of a link that goes down without a
/// route event. Each message is read and dropped as it comes, so a burst is one wake and nothing
/// queues while the caller is busy.
pub fn link_route_and_address_changes() -> Result<impl Stream<Item = ()> + Unpin, Error> {
    use rtnetlink::MulticastGroup;
    let (connection, _, messages) = rtnetlink::new_multicast_connection(&[
        MulticastGroup::Link,
        MulticastGroup::Ipv4Route,
        MulticastGroup::Ipv6Route,
        MulticastGroup::Ipv4Ifaddr,
        MulticastGroup::Ipv6Ifaddr,
    ])
    .map_err(Error::Subscribe)?;
    tokio::spawn(connection);
    Ok(read_as_they_come(messages.map(|_| ())))
}

/// `changes` read as they come and dropped, leaving only that one came.
fn read_as_they_come(
    mut changes: impl Stream<Item = ()> + Send + Unpin + 'static,
) -> impl Stream<Item = ()> + Unpin {
    let (came, seen) = tokio::sync::watch::channel(());
    tokio::spawn(async move {
        while changes.next().await.is_some() {
            if came.send(()).is_err() {
                break;
            }
        }
    });
    Box::pin(
        futures::stream::unfold(seen, |mut seen| async move {
            seen.changed().await.ok().map(|()| ((), seen))
        })
        .fuse(),
    )
}

fn route_message(route: &Route, action: &'static str) -> Result<RouteMessage, Error> {
    let invalid = |reason: &dyn fmt::Display| Error::Invalid {
        action,
        route: route.to_string(),
        reason: reason.to_string(),
    };
    let mut builder = RouteMessageBuilder::<IpAddr>::new()
        .destination_prefix(route.destination.addr(), route.destination.prefix_len())
        .map_err(|e| invalid(&e))?
        .table_id(route.table)
        .protocol(RouteProtocol::from(route.protocol));
    if let Some(gateway) = route.gateway {
        builder = builder.gateway(gateway).map_err(|e| invalid(&e))?;
    }
    if let Some(oif) = route.oif {
        builder = builder.output_interface(oif);
    }
    if let Some(metric) = route.metric {
        builder = builder.priority(metric);
    }
    Ok(builder.build())
}

fn route_of(message: &RouteMessage) -> Option<Route> {
    let mut destination = None;
    let mut table = u32::from(message.header.table);
    let mut gateway = None;
    let mut oif = None;
    let mut metric = None;
    for attribute in &message.attributes {
        match attribute {
            RouteAttribute::Destination(address) => destination = ip_of(address),
            RouteAttribute::Gateway(address) => gateway = ip_of(address),
            // An IPv6 next hop of an IPv4 route.
            RouteAttribute::Via(RouteVia::Inet6(address)) => gateway = Some(IpAddr::V6(*address)),
            // The header holds a table above 255 as `RT_TABLE_COMPAT`; the attribute holds any.
            RouteAttribute::Table(id) => table = *id,
            RouteAttribute::Oif(index) => oif = Some(*index),
            RouteAttribute::Priority(priority) => metric = Some(*priority),
            _ => {}
        }
    }
    // Left out of a default route alone.
    let address = destination.unwrap_or(match message.header.address_family {
        rtnetlink::packet_route::AddressFamily::Inet6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        _ => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    });
    let destination = IpNet::new(address, message.header.destination_prefix_length).ok()?;
    Some(Route {
        destination,
        table,
        protocol: message.header.protocol.into(),
        gateway,
        oif,
        metric,
    })
}

fn ip_of(address: &RouteAddress) -> Option<IpAddr> {
    match address {
        RouteAddress::Inet(address) => Some(IpAddr::V4(*address)),
        RouteAddress::Inet6(address) => Some(IpAddr::V6(*address)),
        _ => None,
    }
}

/// The address `message` gives a packet to leave from: of scope universe, neither tentative nor
/// failed in duplicate address detection. On a point-to-point link `IFA_ADDRESS` is the peer's.
fn source_address(message: &AddressMessage) -> Option<IpAddr> {
    use rtnetlink::packet_route::address::{AddressAttribute, AddressHeaderFlags, AddressScope};
    let unusable = AddressHeaderFlags::Tentative | AddressHeaderFlags::Dadfailed;
    if message.header.scope != AddressScope::Universe || message.header.flags.intersects(unusable) {
        return None;
    }
    let local = message.attributes.iter().find_map(|a| match a {
        AddressAttribute::Local(address) => Some(*address),
        _ => None,
    });
    let address = message.attributes.iter().find_map(|a| match a {
        AddressAttribute::Address(address) => Some(*address),
        _ => None,
    });
    local.or(address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtnetlink::AddressMessageBuilder;
    use rtnetlink::packet_route::address::{AddressAttribute, AddressHeaderFlags, AddressScope};

    fn address(text: &str) -> AddressMessage {
        match text.parse().unwrap() {
            IpNet::V4(net) => AddressMessageBuilder::<Ipv4Addr>::new()
                .index(7)
                .address(net.addr(), net.prefix_len())
                .build(),
            IpNet::V6(net) => AddressMessageBuilder::<Ipv6Addr>::new()
                .index(7)
                .address(net.addr(), net.prefix_len())
                .build(),
        }
    }

    #[test]
    fn a_source_address_is_global_and_neither_tentative_nor_failed() {
        let global = address("2001:db8::5/64");
        assert_eq!(
            source_address(&global),
            Some("2001:db8::5".parse().unwrap())
        );

        let mut deprecated = global.clone();
        deprecated.header.flags = AddressHeaderFlags::Deprecated;
        assert_eq!(source_address(&deprecated), source_address(&global));

        for flags in [AddressHeaderFlags::Tentative, AddressHeaderFlags::Dadfailed] {
            let mut refused = global.clone();
            refused.header.flags = flags | AddressHeaderFlags::Permanent;
            assert_eq!(source_address(&refused), None, "{flags:?}");
        }
        for scope in [AddressScope::Link, AddressScope::Host, AddressScope::Site] {
            let mut refused = global.clone();
            refused.header.scope = scope;
            assert_eq!(source_address(&refused), None, "{scope:?}");
        }
    }

    #[test]
    fn a_point_to_point_address_is_its_local_one_not_the_peers() {
        let mut point_to_point = address("192.0.2.1/32");
        point_to_point.attributes = vec![
            AddressAttribute::Address("192.0.2.1".parse().unwrap()),
            AddressAttribute::Local("192.0.2.5".parse().unwrap()),
        ];
        assert_eq!(
            source_address(&point_to_point),
            Some("192.0.2.5".parse().unwrap())
        );
    }

    #[test]
    fn a_route_reads_back_as_it_was_written_in_either_family_any_table_and_with_either_next_hop() {
        for route in [
            Route {
                destination: "192.0.2.0/24".parse().unwrap(),
                table: 152,
                protocol: 152,
                gateway: Some("198.51.100.1".parse().unwrap()),
                oif: Some(7),
                metric: Some(16),
            },
            Route {
                destination: "192.0.2.0/24".parse().unwrap(),
                table: 152,
                protocol: 152,
                gateway: Some("2001:db8::1".parse().unwrap()),
                oif: Some(7),
                metric: None,
            },
            Route {
                destination: "2001:db8::/32".parse().unwrap(),
                table: 1000,
                protocol: 200,
                gateway: None,
                oif: None,
                metric: None,
            },
        ] {
            assert_eq!(
                route_of(&route_message(&route, "replace").unwrap()),
                Some(route.clone())
            );
        }
    }

    /// A change not read yet, counted in `unread` until it is dropped.
    struct Unread(Arc<std::sync::atomic::AtomicUsize>);

    impl Drop for Unread {
        fn drop(&mut self) {
            self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn a_burst_is_read_as_it_comes_and_wakes_once() {
        let unread = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (tx, rx) = futures::channel::mpsc::unbounded();
        for _ in 0..1001 {
            unread.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tx.unbounded_send(Unread(Arc::clone(&unread))).unwrap();
        }
        drop(tx);
        let changes = read_as_they_come(rx.map(drop));
        tokio::task::yield_now().await;
        assert_eq!(unread.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(changes.collect::<Vec<()>>().await, [()]);
    }
}
