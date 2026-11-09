//! What every slipmesh daemon and tool sets up the same way around its own work.

#[cfg(feature = "serve")]
mod serve;
#[cfg(feature = "serve")]
pub use serve::{Shutdown, serve};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

/// Logs to stdout, filtered by `RUST_LOG`, at INFO without it. No timestamp and no colour: what
/// collects the output, Talos's service log or the container runtime, stamps every line itself and
/// stores it as text.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(filter(std::env::var("RUST_LOG").ok().as_deref()))
        .without_time()
        .with_ansi(false)
        .init();
}

/// INFO and above when `RUST_LOG` is unset or empty: Talos starts an extension service with no
/// environment of its own, and a daemon has to say at least what it is and what it did.
fn filter(rust_log: Option<&str>) -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse_lossy(rust_log.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_rust_log_info_and_above_are_logged() {
        assert_eq!(filter(None).max_level_hint(), Some(LevelFilter::INFO));
        assert_eq!(filter(Some("")).max_level_hint(), Some(LevelFilter::INFO));
    }

    #[test]
    fn rust_log_overrides_the_default() {
        assert_eq!(
            filter(Some("debug")).max_level_hint(),
            Some(LevelFilter::DEBUG)
        );
        assert_eq!(
            filter(Some("error")).max_level_hint(),
            Some(LevelFilter::ERROR)
        );
    }
}
