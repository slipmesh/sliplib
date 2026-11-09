//! What every slipmesh daemon and tool sets up the same way around its own work.

use std::future::Future;
use std::time::Duration;

use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tokio_util::sync::CancellationToken;

/// Logs to stdout, filtered by `RUST_LOG`. No timestamp and no colour: what collects the output,
/// Talos's service log or the container runtime, stamps every line itself and stores it as text.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_ansi(false)
        .init();
}

/// Runs `work` until it returns, and asks it to stop on SIGTERM or SIGINT through the
/// [`Shutdown`] it is given. Work still running `shutdown_timeout` after that request is dropped
/// and `serve` fails, so the timeout has to end before whatever sent the signal gives up and sends
/// SIGKILL.
///
/// The handlers are installed before `work` starts. A failed `work` is logged as `name` with its
/// whole chain, `{:#}`: the log line is all an operator sees of it, and `anyhow`'s plain `Display`
/// is only the outermost context.
pub async fn serve<W, Fut>(
    name: &'static str,
    shutdown_timeout: Duration,
    work: W,
) -> anyhow::Result<()>
where
    W: FnOnce(Shutdown) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    // `Toplevel::new` spawns at once, so on a multi-threaded runtime `work` could otherwise start
    // before `catch_signals` has installed the handlers.
    let signals_caught = CancellationToken::new();
    let start_work = signals_caught.clone();
    let toplevel = Toplevel::new(async move |toplevel: &mut SubsystemHandle| {
        start_work.cancelled().await;
        toplevel.start(SubsystemBuilder::new(
            name,
            async move |subsystem: &mut SubsystemHandle| {
                work(Shutdown(subsystem.create_cancellation_token()))
                    .await
                    .map_err(|e| format!("{e:#}"))
            },
        ));
    })
    .catch_signals();
    signals_caught.cancel();
    toplevel
        .handle_shutdown_requests(shutdown_timeout)
        .await
        .map_err(Into::into)
}

/// The stop request [`serve`] passes to its work.
#[derive(Clone)]
pub struct Shutdown(CancellationToken);

impl Shutdown {
    /// Completes once a stop is requested, at once if it already has been.
    pub async fn requested(&self) {
        self.0.cancelled().await;
    }
}
