//! A test binary of its own, and its one test: it sets the environment `run` reads.

use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use k8s_openapi::api::core::v1::Node;
use kube::runtime::watcher::Event;
use kube::{Api, ResourceExt};
use sliplib_kube::testing::{FakeApi, node};
use sliplib_kube::{ApiTimeouts, run, watch_one};

const TIMEOUTS: ApiTimeouts = ApiTimeouts {
    watch_secs: 1,
    read: Duration::from_secs(2),
};

fn kubeconfig(server: &str) -> String {
    format!(
        "apiVersion: v1
kind: Config
clusters: [{{name: test, cluster: {{server: \"{server}\"}}}}]
users: [{{name: test, user: {{}}}}]
contexts: [{{name: test, context: {{cluster: test, user: test}}}}]
current-context: test
"
    )
}

#[test]
fn the_body_gets_this_nodes_name_and_a_client_of_the_inferred_config_and_none_without_a_name() {
    // One thread: nothing reads the environment while it is set.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let api = FakeApi::start(0, node("node-a", 1, &[]), vec![]).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kubeconfig");
        std::fs::write(&path, kubeconfig(&api.url())).unwrap();
        // SAFETY: no other thread runs.
        unsafe {
            std::env::remove_var("NODE_NAME");
            std::env::set_var("KUBECONFIG", &path);
        }

        let called = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&called);
        let refused = run(
            "test",
            Duration::from_secs(5),
            TIMEOUTS,
            async move |_, _| {
                *flag.lock().unwrap() = true;
                Ok(())
            },
        )
        .await;
        assert!(refused.is_err());
        assert!(!*called.lock().unwrap(), "body run without a node name");

        // SAFETY: no other thread runs.
        unsafe { std::env::set_var("NODE_NAME", "node-a") };
        let seen = Arc::new(Mutex::new(None));
        let found = Arc::clone(&seen);
        run(
            "test",
            Duration::from_secs(5),
            TIMEOUTS,
            async move |agent, _| {
                let nodes: Api<Node> = Api::all(agent.client);
                let mut events = pin!(watch_one(nodes, &agent.node, &TIMEOUTS));
                while let Some(event) = events.next().await {
                    if let Ok(Event::InitApply(node)) = event {
                        *found.lock().unwrap() = Some((agent.node.clone(), node.name_any()));
                        return Ok(());
                    }
                }
                anyhow::bail!("the watch ended")
            },
        )
        .await
        .unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            Some(("node-a".to_owned(), "node-a".to_owned()))
        );
    });
}
