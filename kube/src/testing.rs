//! A Kubernetes API stand-in for the tests of the agents built on this crate.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Answers a list with one object, and the first watch with a list of changes, holding every
/// watch open after that. The first `unanswered` connections are read and never answered, as an
/// API server that goes down mid-request leaves them.
pub struct FakeApi {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeApi {
    /// `listed` is the object every list gives; each change is sent as `MODIFIED`, after its
    /// delay.
    pub async fn start(unanswered: usize, listed: Value, changes: Vec<(Duration, Value)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(serve(
            listener,
            unanswered,
            listed,
            changes,
            Arc::clone(&requests),
        ));
        Self { url, requests }
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    pub fn config(&self) -> kube::Config {
        kube::Config::new(self.url.parse().unwrap())
    }

    /// The head of every request so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// A `Node` called `name` at `resource_version`, with `annotations`.
pub fn node(name: &str, resource_version: u32, annotations: &[(&str, &str)]) -> Value {
    let annotations: serde_json::Map<String, Value> = annotations
        .iter()
        .map(|(key, value)| ((*key).to_owned(), json!(value)))
        .collect();
    json!({
        "apiVersion": "v1",
        "kind": "Node",
        "metadata": {
            "name": name,
            "resourceVersion": resource_version.to_string(),
            "annotations": annotations,
        },
    })
}

async fn serve(
    listener: TcpListener,
    mut unanswered: usize,
    listed: Value,
    mut changes: Vec<(Duration, Value)>,
    requests: Arc<Mutex<Vec<String>>>,
) {
    let mut held = Vec::new();
    loop {
        let (mut conn, _) = listener.accept().await.unwrap();
        let Some(request) = read_request(&mut conn).await else {
            continue;
        };
        requests.lock().unwrap().push(request.clone());
        if unanswered > 0 {
            unanswered -= 1;
            held.push(conn);
            continue;
        }
        if request.contains("watch=true") {
            conn.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\n\r\n")
                .await
                .unwrap();
            for (delay, change) in changes.drain(..) {
                tokio::time::sleep(delay).await;
                let event = json!({"type": "MODIFIED", "object": change}).to_string() + "\n";
                let chunk = format!("{:x}\r\n{event}\r\n", event.len());
                conn.write_all(chunk.as_bytes()).await.unwrap();
            }
            held.push(conn);
            continue;
        }
        let body = json!({
            "apiVersion": listed["apiVersion"],
            "kind": format!("{}List", listed["kind"].as_str().unwrap_or_default()),
            "metadata": {"resourceVersion": listed["metadata"]["resourceVersion"]},
            "items": [listed],
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        conn.write_all(response.as_bytes()).await.unwrap();
    }
}

fn head_complete(request: &[u8]) -> bool {
    request.windows(4).any(|w| w == b"\r\n\r\n")
}

/// `None` if the client closes the connection before a whole request head arrives.
async fn read_request(conn: &mut TcpStream) -> Option<String> {
    let mut request = Vec::new();
    let mut buf = [0; 4096];
    while !head_complete(&request) {
        let n = conn.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        request.extend_from_slice(&buf[..n]);
    }
    Some(String::from_utf8(request).unwrap())
}
