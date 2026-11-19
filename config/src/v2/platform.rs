//! The bodies of the documents written for one platform, as far as they are keys of
//! `slipmesh.yaml`: the settings of a `platform` document and the keys of a `template`. The text of
//! a template and the body of a `patch` are in the platform's own language, and its tool's to read.

use serde::Deserialize;
use std::num::NonZeroU16;

/// The settings of `talos`: the body of its `platform` document.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct TalosSettings {
    #[serde(default)]
    pub metrics: Option<Metrics>,
}

/// The Prometheus metrics of every `talos` node, by what serves them. An entry left out means no
/// listener.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(try_from = "MetricsKeys")]
pub struct Metrics {
    /// The node's links.
    pub mesh: Option<Listener>,
    /// The node's routing.
    pub router: Option<Listener>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricsKeys {
    #[serde(default)]
    mesh: Option<Listener>,
    #[serde(default)]
    router: Option<Listener>,
}

/// What is wrong with the metrics of `talos`.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MetricsError {
    #[error(
        "`metrics.mesh` and `metrics.router` both take port {0}, and two daemons cannot serve one port - give each its own"
    )]
    OnePort(u16),
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    /// The TCP port the metrics are served on, on the node's IPv4 loopback.
    pub port: NonZeroU16,
}

impl TryFrom<MetricsKeys> for Metrics {
    type Error = MetricsError;

    fn try_from(keys: MetricsKeys) -> Result<Self, MetricsError> {
        if let (Some(mesh), Some(router)) = (&keys.mesh, &keys.router)
            && mesh.port == router.port
        {
            return Err(MetricsError::OnePort(mesh.port.get()));
        }
        Ok(Self {
            mesh: keys.mesh,
            router: keys.router,
        })
    }
}

/// The settings of `routeros`: none, so any key in the body of its `platform` document is refused.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct RouterosSettings {}

/// The daemon on a node that makes its own configuration from a template.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Daemon {
    Router,
    Firewall,
}

/// The body of a `template` document.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub daemon: Daemon,
    pub base: Base,
    /// The text of the template, which the daemon's own tool checks.
    pub content: String,
}

/// The reference template a text was made from, by the SHA-256 of that reference's text.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Base(String);

impl Base {
    /// The 64 hexadecimal digits of the hash, in lower case whatever case the files write.
    pub fn digest(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "`base` is `sha256:` and the 64 hexadecimal digits of the SHA-256 of the reference template, which {0:?} is not - `slipmesh-taloscfg template` prints the document with it"
)]
pub struct BaseError(String);

impl TryFrom<String> for Base {
    type Error = BaseError;

    fn try_from(text: String) -> Result<Self, BaseError> {
        match text.strip_prefix("sha256:") {
            Some(digest) if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) => {
                Ok(Self(digest.to_ascii_lowercase()))
            }
            _ => Err(BaseError(text)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read<T: serde::de::DeserializeOwned>(yaml: &str) -> Result<T, String> {
        sliplib_yaml::from_str(yaml).map_err(|error| error.to_string())
    }

    fn port(number: u16) -> Option<Listener> {
        Some(Listener {
            port: NonZeroU16::new(number).unwrap(),
        })
    }

    #[test]
    fn the_settings_of_talos_hold_a_port_for_each_listener_written() {
        let both: TalosSettings =
            read("metrics: {mesh: {port: 9586}, router: {port: 9324}}").unwrap();
        let metrics = both.metrics.unwrap();
        assert_eq!((metrics.mesh, metrics.router), (port(9586), port(9324)));

        let one: TalosSettings = read("metrics: {router: {port: 9324}}").unwrap();
        let metrics = one.metrics.unwrap();
        assert_eq!((metrics.mesh, metrics.router), (None, port(9324)));

        assert_eq!(read::<TalosSettings>("{}").unwrap().metrics, None);
    }

    #[test]
    fn two_listeners_on_one_port_are_refused_naming_it() {
        let refused = read::<TalosSettings>("metrics: {mesh: {port: 9586}, router: {port: 9586}}")
            .unwrap_err();
        assert!(
            refused.contains(&MetricsError::OnePort(9586).to_string()),
            "{refused}"
        );
    }

    #[test]
    fn a_listener_needs_a_port_that_is_not_zero() {
        assert!(read::<TalosSettings>("metrics: {mesh: {port: 1}}").is_ok());
        for yaml in ["metrics: {mesh: {}}", "metrics: {mesh: {port: 0}}"] {
            assert!(read::<TalosSettings>(yaml).is_err(), "{yaml}");
        }
    }

    #[test]
    fn a_key_the_settings_do_not_have_is_refused_at_any_depth() {
        assert!(read::<TalosSettings>("metrics: {mesh: {port: 1}}").is_ok());
        for yaml in [
            "{metrics: {mesh: {port: 1}}, listen: true}",
            "metrics: {mesh: {port: 1}, firewall: {port: 2}}",
            "metrics: {mesh: {port: 1, address: 192.0.2.1}}",
        ] {
            assert!(read::<TalosSettings>(yaml).is_err(), "{yaml}");
        }
    }

    #[test]
    fn routeros_has_no_settings() {
        assert_eq!(read::<RouterosSettings>("{}").unwrap(), RouterosSettings {});
        assert!(read::<RouterosSettings>("metrics: {}").is_err());
    }

    const DIGEST: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn a_template_names_its_daemon_its_base_and_its_text() {
        let template: Template = read(&format!(
            "daemon: firewall\nbase: sha256:{DIGEST}\ncontent: |\n  table inet t {{}}\n"
        ))
        .unwrap();
        assert_eq!(template.daemon, Daemon::Firewall);
        assert_eq!(template.base.digest(), DIGEST);
        assert_eq!(template.content, "table inet t {}\n");

        let router: Template = read(&format!(
            "{{daemon: router, base: 'sha256:{DIGEST}', content: x}}"
        ))
        .unwrap();
        assert_eq!(router.daemon, Daemon::Router);
    }

    #[test]
    fn a_template_refuses_another_daemon_a_missing_key_and_an_unknown_one() {
        let base = format!("base: 'sha256:{DIGEST}'");
        assert!(read::<Template>(&format!("{{daemon: router, {base}, content: x}}")).is_ok());
        for yaml in [
            format!("{{daemon: awg, {base}, content: x}}"),
            format!("{{{base}, content: x}}"),
            "{daemon: router, content: x}".to_owned(),
            format!("{{daemon: router, {base}}}"),
            format!("{{daemon: router, {base}, content: x, name: y}}"),
        ] {
            assert!(read::<Template>(&yaml).is_err(), "{yaml}");
        }
    }

    #[test]
    fn a_base_is_read_in_either_case_and_held_in_lower() {
        let upper = format!("sha256:{}", DIGEST.to_ascii_uppercase());
        assert_eq!(Base::try_from(upper).unwrap().digest(), DIGEST);
    }

    #[test]
    fn a_base_that_is_not_a_sha256_is_refused_naming_what_prints_one() {
        for text in [
            DIGEST.to_owned(),
            format!("sha512:{DIGEST}"),
            format!("sha256:{}", &DIGEST[1..]),
            format!("sha256:{DIGEST}0"),
            format!("sha256:{}g", &DIGEST[1..]),
        ] {
            assert_eq!(Base::try_from(text.clone()), Err(BaseError(text)));
        }
    }
}
