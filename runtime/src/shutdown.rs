use std::io;
use tokio::signal::unix::{Signal, SignalKind, signal};

/// SIGTERM is how Talos stops an extension service and how the kubelet stops a container. A daemon
/// runs as PID 1 in its container, and the kernel drops a signal PID 1 has no handler for, so
/// without one every stop waits out the grace period and ends in SIGKILL.
pub struct Shutdown(Signal);

impl Shutdown {
    /// Install before anything that can take time: a SIGTERM that arrives earlier is lost.
    ///
    /// Panics outside a Tokio runtime, like every Tokio signal handler.
    pub fn install() -> io::Result<Self> {
        signal(SignalKind::terminate()).map(Self)
    }

    /// Resolves once SIGTERM has arrived, including one that came before the first call.
    pub async fn requested(&mut self) {
        self.0.recv().await;
    }
}
