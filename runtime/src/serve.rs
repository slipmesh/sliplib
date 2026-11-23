use std::future::Future;
use std::time::Duration;

use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};
use tokio_util::sync::CancellationToken;

/// Asks `work` to stop on SIGTERM or SIGINT, and drops it and fails `shutdown_timeout` later: keep
/// that below the sender's wait before SIGKILL. A failure is logged with `{:#}`, its whole chain:
/// plain `Display` is only the outermost context.
pub async fn serve<W, Fut>(
    name: &'static str,
    shutdown_timeout: Duration,
    work: W,
) -> anyhow::Result<()>
where
    W: FnOnce(Shutdown) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    // `Toplevel::new` spawns at once: `work` could start before the handlers are in.
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

#[derive(Clone)]
pub struct Shutdown(CancellationToken);

impl Shutdown {
    /// At once if already requested.
    pub async fn requested(&self) {
        self.0.cancelled().await;
    }
}
