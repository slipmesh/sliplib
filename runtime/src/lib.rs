//! What every slipmesh daemon and tool sets up the same way around its own work.

#[cfg(feature = "serve")]
mod serve;
#[cfg(feature = "serve")]
pub use serve::{Shutdown, serve};

/// Logs to stdout, filtered by `RUST_LOG`. No timestamp and no colour: what collects the output,
/// Talos's service log or the container runtime, stamps every line itself and stores it as text.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_ansi(false)
        .init();
}
