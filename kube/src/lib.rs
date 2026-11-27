//! The start every agent in the cluster shares: the client, its own node, the stop, and a watch of
//! one object. TLS and the Kubernetes version are the binary's: it enables kube's TLS feature with
//! a crypto provider, `ring` or `aws-lc-rs` (rustls without one panics at runtime), or keeps kube's
//! defaults, and one `k8s-openapi` version feature.

#[cfg(feature = "testing")]
pub mod testing;

use std::fmt::Debug;
use std::future::Future;
use std::time::Duration;

use anyhow::Context;
use futures::Stream;
use kube::runtime::{WatchStreamExt, watcher};
use kube::{Api, Resource};
use serde::de::DeserializeOwned;
use sliplib_runtime::Shutdown;

/// Where the pod is given its node's name, from `spec.nodeName` through the Downward API.
pub const NODE_NAME: &str = "NODE_NAME";

/// kube sets no response timeout and bounds only an open watch stream, so a request the API server
/// accepted but never answers would hold a watch forever. `read` has to outlast `watch_secs`, the
/// server-side lifetime of a watch, or every quiet watch would be cut as dead.
#[derive(Debug, Clone, Copy)]
pub struct ApiTimeouts {
    pub watch_secs: u32,
    pub read: Duration,
}

impl ApiTimeouts {
    pub fn client(&self, mut config: kube::Config) -> Result<kube::Client, kube::Error> {
        config.read_timeout = Some(self.read);
        // Or the time a pooled connection sat idle counts against the next response.
        config.reset_reader_on_write = true;
        kube::Client::try_from(config)
    }
}

/// What every agent starts from.
pub struct Agent {
    pub client: kube::Client,
    pub node: String,
}

/// Runs `body` under `sliplib_runtime::serve`, with a client of the inferred config and this
/// node's name, logged once known. Setting up logging is the caller's, first in `main`.
pub async fn run<B, Fut>(
    name: &'static str,
    shutdown_timeout: Duration,
    timeouts: ApiTimeouts,
    body: B,
) -> anyhow::Result<()>
where
    B: FnOnce(Agent, Shutdown) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    sliplib_runtime::serve(name, shutdown_timeout, async move |shutdown| {
        let node = std::env::var(NODE_NAME).with_context(|| {
            format!("{NODE_NAME} is not set: give it spec.nodeName through the Downward API")
        })?;
        tracing::info!(name, node, "starting");
        let config = kube::Config::infer()
            .await
            .context("failed to load the Kubernetes config")?;
        let client = timeouts
            .client(config)
            .context("failed to build the Kubernetes client")?;
        body(Agent { client, node }, shutdown).await
    })
    .await
}

/// Every change to the one object called `name`, selected by the API server, with kube's default
/// backoff between failed requests.
pub fn watch_one<K>(
    api: Api<K>,
    name: &str,
    timeouts: &ApiTimeouts,
) -> impl Stream<Item = watcher::Result<watcher::Event<K>>> + Send + use<K>
where
    K: Resource + Clone + DeserializeOwned + Debug + Send + 'static,
{
    let config = watcher::Config::default()
        .fields(&format!("metadata.name={name}"))
        .timeout(timeouts.watch_secs);
    watcher(api, config).default_backoff()
}
