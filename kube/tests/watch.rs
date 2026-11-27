use std::pin::pin;
use std::time::Duration;

use futures::StreamExt;
use k8s_openapi::api::core::v1::Node;
use kube::runtime::watcher::Event;
use kube::{Api, ResourceExt};
use sliplib_kube::testing::{FakeApi, node};
use sliplib_kube::{ApiTimeouts, watch_one};

const TIMEOUTS: ApiTimeouts = ApiTimeouts {
    watch_secs: 1,
    read: Duration::from_secs(2),
};

/// `watch_one` for `node-a` against `api`, polled in a task of its own: the first object it gives,
/// or `None` within `deadline`.
async fn first_object(api: &FakeApi, deadline: Duration) -> Option<Node> {
    let nodes: Api<Node> = Api::all(TIMEOUTS.client(api.config()).unwrap());
    let (found, mut objects) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut events = pin!(watch_one(nodes, "node-a", &TIMEOUTS));
        while let Some(event) = events.next().await {
            if let Ok(Event::InitApply(node) | Event::Apply(node)) = event {
                let _ = found.send(node);
            }
        }
    });
    tokio::time::timeout(deadline, objects.recv())
        .await
        .ok()
        .flatten()
}

#[tokio::test]
async fn asks_only_for_the_named_object_and_bounds_each_watch() {
    let api = FakeApi::start(0, node("node-a", 1, &[]), vec![]).await;
    let found = first_object(&api, Duration::from_secs(5)).await.unwrap();
    assert_eq!(found.name_any(), "node-a");

    tokio::time::sleep(Duration::from_millis(200)).await;
    let requests = api.requests();
    assert!(
        requests
            .iter()
            .any(|request| request.contains("watch=true")),
        "{requests:?}"
    );
    for request in &requests {
        assert!(
            request.contains("fieldSelector=metadata.name%3Dnode-a"),
            "{request}"
        );
        if request.contains("watch=true") {
            assert!(request.contains("timeoutSeconds=1"), "{request}");
        }
    }
}

#[tokio::test]
async fn recovers_from_a_request_the_api_never_answers_once_the_read_times_out() {
    let api = FakeApi::start(1, node("node-a", 1, &[]), vec![]).await;
    let start = tokio::time::Instant::now();
    assert!(
        first_object(&api, Duration::from_secs(10)).await.is_some(),
        "nothing once the API answers"
    );
    assert!(start.elapsed() >= TIMEOUTS.read, "{:?}", start.elapsed());
}
