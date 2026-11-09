//! A test binary of its own: a signal goes to the whole process.

mod common;

use std::time::Duration;

#[tokio::test]
async fn work_that_outlasts_the_shutdown_timeout_fails_serve() {
    let served = tokio::time::timeout(
        Duration::from_secs(5),
        sliplib_runtime::serve("test", Duration::from_millis(100), async |_| {
            common::send_sigterm_to_this_process()?;
            std::future::pending().await
        }),
    )
    .await
    .expect("serve waited past its shutdown timeout");
    assert!(served.is_err());
}
