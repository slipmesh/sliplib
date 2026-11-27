#![cfg(target_os = "linux")]

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use prometheus_client::metrics::counter::Counter;
use prometheus_client::registry::Registry;
use sliplib_runtime::metrics::{self, OPENMETRICS, Page};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// `/metrics` with one counter, `/prefixes` as text, `/broken` failing, nothing else, at most
/// `connections` at once; and how often a page was rendered.
async fn serving(connections: NonZeroUsize) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let rendered = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&rendered);
    tokio::spawn(metrics::serve(listener, connections, move |path: &str| {
        count.fetch_add(1, Ordering::SeqCst);
        match path {
            "/metrics" => Some(Box::pin(async {
                let mut registry = Registry::default();
                registry.register("scrapes", "Scrapes", Counter::<u64>::default());
                Page::openmetrics(&registry).map_err(|e| e.to_string())
            }) as metrics::Rendering<String>),
            "/prefixes" => Some(Box::pin(async {
                Ok(Page::text("192.0.2.0/24\n".to_owned()))
            })),
            "/broken" => Some(Box::pin(async { Err("broken".to_owned()) })),
            _ => None,
        }
    }));
    (address, rendered)
}

async fn send(stream: &mut TcpStream, method: &str, path: &str, close: bool) {
    let connection = if close { "close" } else { "keep-alive" };
    stream
        .write_all(
            format!(
                "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: {connection}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
}

async fn request(address: SocketAddr, method: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    send(&mut stream, method, path, true).await;
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn each_served_path_answers_with_its_page_and_its_content_type() {
    let (address, _) = serving(NonZeroUsize::new(4).unwrap()).await;
    let response = request(address, "GET", "/metrics").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response.contains(&format!("content-type: {OPENMETRICS}")),
        "{response}"
    );
    assert!(response.contains("scrapes_total 0"), "{response}");

    let response = request(address, "GET", "/prefixes").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\n192.0.2.0/24\n"), "{response}");
}

#[tokio::test]
async fn another_path_or_method_is_a_404_and_renders_nothing() {
    let (address, rendered) = serving(NonZeroUsize::new(4).unwrap()).await;
    let response = request(address, "POST", "/metrics").await;
    assert!(response.starts_with("HTTP/1.1 404 Not Found"), "{response}");
    assert_eq!(rendered.load(Ordering::SeqCst), 0);
    let response = request(address, "GET", "/").await;
    assert!(response.starts_with("HTTP/1.1 404 Not Found"), "{response}");
}

#[tokio::test]
async fn a_page_that_fails_to_render_is_a_500() {
    let (address, _) = serving(NonZeroUsize::new(4).unwrap()).await;
    let response = request(address, "GET", "/broken").await;
    assert!(
        response.starts_with("HTTP/1.1 500 Internal Server Error"),
        "{response}"
    );
}

#[tokio::test]
async fn a_connection_past_the_cap_waits_and_an_idle_one_is_cut() {
    let (address, _) = serving(NonZeroUsize::MIN).await;
    let silent = TcpStream::connect(address).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut waiting = TcpStream::connect(address).await.unwrap();
    send(&mut waiting, "GET", "/metrics", false).await;
    let mut buf = [0; 1];
    let early = tokio::time::timeout(Duration::from_millis(300), waiting.read(&mut buf)).await;
    assert!(early.is_err(), "served past the cap");

    let mut response = String::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        waiting.read_to_string(&mut response),
    )
    .await
    .expect("still waiting after the silent client's header timeout")
    .unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    drop(silent);
}

/// An address on no interface yet, such as a loopback another service has still to create.
#[tokio::test]
async fn binds_an_address_of_either_family_that_does_not_exist_on_any_interface() {
    for listen in ["198.51.100.126:0", "[2001:db8::126]:0"] {
        let listen: SocketAddr = listen.parse().unwrap();
        let listener = metrics::bind(listen).unwrap();
        assert_eq!(listener.local_addr().unwrap().ip(), listen.ip());
    }
}
