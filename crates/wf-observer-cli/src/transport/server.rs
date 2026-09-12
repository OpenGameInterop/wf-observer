use anyhow::Context as _;
use iroh::{
    Endpoint, SecretKey,
    endpoint::{QuicTransportConfig, presets},
    protocol::Router,
};
use tokio_util::task::TaskTracker;

use super::connection::{Handler, SubscriptionLimits};
use crate::service::ServiceView;

const SEND_WINDOW_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) struct Server {
    router: Router,
    requests: TaskTracker,
}

impl Server {
    pub(crate) fn endpoint(&self) -> &Endpoint {
        self.router.endpoint()
    }

    pub(crate) async fn shutdown(self) -> anyhow::Result<()> {
        let result = self
            .router
            .shutdown()
            .await
            .context("failed to shut down the Iroh router");
        self.requests.close();
        self.requests.wait().await;
        result
    }
}

pub(crate) async fn start(secret_key: SecretKey, view: ServiceView) -> anyhow::Result<Server> {
    start_with_limits(secret_key, view, SubscriptionLimits::default()).await
}

pub(super) async fn start_with_limits(
    secret_key: SecretKey,
    view: ServiceView,
    limits: SubscriptionLimits,
) -> anyhow::Result<Server> {
    let message_bytes = view.message_bytes();
    let transport = QuicTransportConfig::builder()
        .max_concurrent_bidi_streams(32_u32.into())
        .max_concurrent_uni_streams(0_u32.into())
        .stream_receive_window(message_bytes.into())
        .receive_window((4 * message_bytes).into())
        .send_window(SEND_WINDOW_BYTES)
        .build();
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .transport_config(transport)
        .bind()
        .await
        .context("failed to bind the Iroh endpoint")?;
    let requests = TaskTracker::new();
    let handler = Handler::new(view, message_bytes, requests.clone(), limits);
    let router = Router::builder(endpoint)
        .accept(protocol::v1::ALPN_V1, handler)
        .spawn();
    Ok(Server { router, requests })
}
