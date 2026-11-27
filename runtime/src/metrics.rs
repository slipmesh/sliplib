//! A daemon's Prometheus endpoint: pages by path, rendered on each request.

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{CONTENT_TYPE, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use prometheus_client::registry::Registry;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;

/// What prometheus-client's text encoder writes. Prometheus picks its parser by this header.
pub const OPENMETRICS: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

const TEXT: &str = "text/plain; charset=utf-8";

/// A client that stops reading holds no task longer than this.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Page {
    pub content_type: &'static str,
    pub body: String,
}

impl Page {
    pub fn openmetrics(registry: &Registry) -> Result<Self, std::fmt::Error> {
        let mut body = String::new();
        prometheus_client::encoding::text::encode(&mut body, registry)?;
        Ok(Self {
            content_type: OPENMETRICS,
            body,
        })
    }
}

/// Binds `listen` even while the address is on no interface yet, such as a loopback another
/// service creates; it answers once the address appears. `IP_FREEBIND` has no tokio API and needs
/// no capability.
pub fn bind(listen: SocketAddr) -> std::io::Result<TcpListener> {
    let socket = Socket::new(
        Domain::for_address(listen),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    if listen.is_ipv4() {
        socket.set_freebind_v4(true)?;
    } else {
        socket.set_freebind_v6(true)?;
    }
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&listen.into())?;
    socket.listen(128)?;
    TcpListener::from_std(socket.into())
}

/// Answers `GET` on every path `pages` gives a renderer for, until the process ends; any other
/// request is a 404 that renders nothing. A page that fails to render is a 500 and is logged: a
/// failed scrape is the signal, and the daemon goes on.
pub async fn serve<P, F>(listener: TcpListener, pages: P)
where
    P: Fn(&str) -> Option<F> + Send + Sync + 'static,
    F: Future<Output = anyhow::Result<Page>> + Send + 'static,
{
    let pages = Arc::new(pages);
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                // One failed accept says nothing about the next.
                tracing::warn!(error = %e, "metrics: accept failed");
                continue;
            }
        };
        let pages = Arc::clone(&pages);
        tokio::spawn(async move {
            let service = service_fn(move |request: Request<Incoming>| {
                let page = (request.method() == Method::GET)
                    .then(|| pages(request.uri().path()))
                    .flatten();
                async move { Ok::<_, Infallible>(answer(page).await) }
            });
            let served = tokio::time::timeout(
                CONNECTION_TIMEOUT,
                http1::Builder::new().serve_connection(TokioIo::new(stream), service),
            )
            .await;
            match served {
                Ok(Err(e)) => tracing::debug!(error = %e, "metrics: connection ended in error"),
                Err(_) => tracing::debug!("metrics: connection timed out"),
                Ok(Ok(())) => {}
            }
        });
    }
}

async fn answer(page: Option<impl Future<Output = anyhow::Result<Page>>>) -> Response<Full<Bytes>> {
    let Some(page) = page else {
        return respond(StatusCode::NOT_FOUND, TEXT, "not found\n".to_owned());
    };
    match page.await {
        Ok(page) => respond(StatusCode::OK, page.content_type, page.body),
        Err(e) => {
            tracing::warn!(error = %format_args!("{e:#}"), "metrics: rendering failed");
            respond(StatusCode::INTERNAL_SERVER_ERROR, TEXT, String::new())
        }
    }
}

fn respond(status: StatusCode, content_type: &'static str, body: String) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}
