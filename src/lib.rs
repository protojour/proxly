use std::sync::Arc;

use authly::{authly_tls_client_config, authly_tls_server_config};
use state::{new_hickory, ProxlyState};
use tokio::net::TcpListener;
use tracing::{info, info_span, Instrument};

mod authly;
mod egress;
mod ingress;
mod ip_util;
mod state;

pub async fn run_proxy() -> anyhow::Result<()> {
    let cancel = tower_server::signal::termination_signal();
    let authly_client = authly_client::Client::builder()
        .from_environment()
        .await?
        .connect()
        .await?;

    let metadata = authly_client.metadata().await?;
    let entity_id = metadata.entity_id();
    let label = metadata.label();

    let state = Arc::new(ProxlyState {
        ingress_tls_config: authly_tls_server_config(&authly_client, cancel.clone()).await?,
        egress_tls_config: authly_tls_client_config(&authly_client, cancel.clone()).await?,
        hickory: new_hickory()?,
        cancel: cancel.clone(),
    });

    let ingress_listener = TcpListener::bind("0.0.0.0:4645").await?;
    let egress_listener = TcpListener::bind("0.0.0.0:4647").await?;

    tokio::spawn(
        ingress::ingress_proxy_tcp_listener_task(ingress_listener, state.clone())
            .instrument(info_span!("ingress")),
    );
    tokio::spawn(
        egress::egress_proxy_tcp_listener_task(egress_listener, state)
            .instrument(info_span!("egress")),
    );

    info!(?entity_id, ?label, "service proxy spawned");

    cancel.cancelled().await;

    Ok(())
}
