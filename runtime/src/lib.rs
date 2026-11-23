//! Logging and shutdown, set up alike for every daemon and tool.

#[cfg(feature = "serve")]
mod serve;
#[cfg(feature = "serve")]
pub use serve::{Shutdown, serve};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

/// To stdout, by `RUST_LOG`. No timestamp or colour: Talos's service log and the container runtime
/// stamp each line and store it as text.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(filter(std::env::var("RUST_LOG").ok().as_deref()))
        .without_time()
        .with_ansi(false)
        .init();
}

/// INFO where `RUST_LOG` is unset or empty: Talos starts an extension service with no environment.
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
