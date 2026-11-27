use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use prometheus_client::metrics::counter::Counter;
use prometheus_client::registry::Registry;
use sliplib_runtime::metrics::{self, OPENMETRICS, Page};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// `/metrics` with one counter, `/broken` failing, nothing else; and how often a page was
/// rendered.
async fn serving() -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let rendered = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&rendered);
    tokio::spawn(metrics::serve(listener, move |path: &str| {
        let count = Arc::clone(&count);
        let page = match path {
            "/metrics" => Ok(()),
            "/broken" => Err(()),
            _ => return None,
        };
        Some(async move {
            count.fetch_add(1, Ordering::SeqCst);
            page.map_err(|()| anyhow::anyhow!("broken"))?;
            let mut registry = Registry::default();
            registry.register("scrapes", "Scrapes", Counter::<u64>::default());
            Ok(Page::openmetrics(&registry)?)
        })
    }));
    (address, rendered)
}

async fn request(address: SocketAddr, method: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn a_served_path_answers_with_its_page_as_openmetrics() {
    let (address, _) = serving().await;
    let response = request(address, "GET", "/metrics").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response.contains(&format!("content-type: {OPENMETRICS}")),
        "{response}"
    );
    assert!(response.contains("scrapes_total 0"), "{response}");
}

#[tokio::test]
async fn another_path_or_method_is_a_404_and_renders_nothing() {
    let (address, rendered) = serving().await;
    for (method, path) in [("GET", "/"), ("POST", "/metrics")] {
        let response = request(address, method, path).await;
        assert!(
            response.starts_with("HTTP/1.1 404 Not Found"),
            "{method} {path}: {response}"
        );
    }
    assert_eq!(rendered.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_page_that_fails_to_render_is_a_500_and_the_next_request_is_served() {
    let (address, _) = serving().await;
    let response = request(address, "GET", "/broken").await;
    assert!(
        response.starts_with("HTTP/1.1 500 Internal Server Error"),
        "{response}"
    );
    let response = request(address, "GET", "/metrics").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
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
