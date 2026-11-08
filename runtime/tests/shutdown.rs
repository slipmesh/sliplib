//! A test binary of its own: the SIGTERM raised here goes to the whole process.
#![cfg(unix)]

use nix::sys::signal::{Signal, raise};
use sliplib_runtime::shutdown::Shutdown;
use std::time::Duration;

#[tokio::test]
async fn sigterm_is_reported_instead_of_ending_the_process() {
    let mut shutdown = Shutdown::install().unwrap();
    raise(Signal::SIGTERM).unwrap();
    tokio::time::timeout(Duration::from_secs(5), shutdown.requested())
        .await
        .expect("SIGTERM was raised but never reported");
}
