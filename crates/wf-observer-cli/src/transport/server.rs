use anyhow::Context as _;
use iroh::{
    Endpoint, EndpointAddr, SecretKey,
    endpoint::{BindOpts, NetReportConfig, PortmapperConfig, QuicTransportConfig, presets},
    protocol::Router,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use tokio_util::task::TaskTracker;

use super::connection::{Handler, SubscriptionLimits};
use crate::{service::ServiceView, settings::AccessMode};

const SEND_WINDOW_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) struct Server {
    router: Router,
    requests: TaskTracker,
}

impl Server {
    pub(crate) fn endpoint(&self) -> &Endpoint {
        self.router.endpoint()
    }

    /// A direct loopback ticket for this run, including when remote access is enabled.
    pub(crate) fn local_ticket(&self) -> String {
        let address = self.endpoint().bound_sockets().into_iter().fold(
            EndpointAddr::new(self.endpoint().id()),
            |address, mut socket| {
                if socket.ip().is_unspecified() {
                    socket.set_ip(match socket.ip() {
                        IpAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
                        IpAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
                    });
                }
                address.with_ip_addr(socket)
            },
        );
        iroh_tickets::endpoint::EndpointTicket::new(address).to_string()
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

pub(crate) async fn start(
    secret_key: SecretKey,
    view: ServiceView,
    mode: AccessMode,
) -> anyhow::Result<Server> {
    start_with_limits(secret_key, view, mode, SubscriptionLimits::default()).await
}

pub(super) async fn start_with_limits(
    secret_key: SecretKey,
    view: ServiceView,
    mode: AccessMode,
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
    let builder = match mode {
        AccessMode::Local => Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .bind_addr((Ipv4Addr::LOCALHOST, 0))?
            .bind_addr_with_opts(
                (Ipv6Addr::LOCALHOST, 0),
                BindOpts::default().set_is_required(false),
            )?
            .portmapper_config(PortmapperConfig::Disabled)
            .net_report_config(NetReportConfig::minimal()),
        AccessMode::Remote => Endpoint::builder(presets::N0),
    };
    let endpoint = builder
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
