//! A test binary of its own: a signal goes to the whole process.

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::time::Duration;

use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread")]
async fn a_sigterm_asks_the_work_to_stop_and_serve_ends_cleanly() {
    sliplib_runtime::serve("test", Duration::from_secs(5), async |shutdown| {
        kill(Pid::this(), Signal::SIGTERM)?;
        timeout(Duration::from_secs(5), shutdown.requested())
            .await
            .map_err(|_| anyhow::anyhow!("no shutdown request after a SIGTERM"))
    })
    .await
    .unwrap();
}
