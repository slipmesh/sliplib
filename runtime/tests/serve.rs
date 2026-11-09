use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, anyhow};

const TIMEOUT: Duration = Duration::from_secs(1);

#[tokio::test]
async fn work_that_ends_ends_serve() {
    sliplib_runtime::serve("test", TIMEOUT, async |_| Ok(()))
        .await
        .unwrap();
}

#[tokio::test]
async fn the_work_is_not_asked_to_stop_without_a_signal() {
    sliplib_runtime::serve("test", TIMEOUT, async |shutdown| {
        let early = tokio::time::timeout(Duration::from_millis(100), shutdown.requested()).await;
        anyhow::ensure!(early.is_err(), "asked to stop with no signal sent");
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn failed_work_fails_serve_and_logs_its_whole_chain() {
    let log = Log::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(log.clone())
            .with_ansi(false)
            .finish(),
    );

    let served = sliplib_runtime::serve("test", TIMEOUT, async |_| {
        Err(anyhow!("no such file")).context("failed to read the template")
    })
    .await;

    assert!(served.is_err());
    let log = log.text();
    assert!(
        log.contains("failed to read the template: no such file"),
        "{log}"
    );
}

#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl Log {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Log {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
