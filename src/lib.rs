use std::sync::Arc;

use authly::authly_tls_config;
use state::ProxlyState;
use tokio::net::TcpListener;

mod authly;
mod ingress;
mod state;

pub async fn run_proxy() -> anyhow::Result<()> {
    let cancel = tower_server::signal::termination_signal();
    let authly_client = authly_client::Client::builder()
        .from_environment()
        .await?
        .connect()
        .await?;

    let state = Arc::new(ProxlyState {
        ingress_fixed_dst_addr: None,
        ingress_tls_config: Some(authly_tls_config(&authly_client, cancel.clone()).await?),
        cancel: cancel.clone(),
    });

    let ingress_listener = TcpListener::bind("0.0.0.0:4645").await?;
    let _egress_listener = TcpListener::bind("0.0.0.0:4647").await?;

    tokio::spawn(ingress::ingress_proxy_tcp_listener_task(
        ingress_listener,
        state,
    ));

    cancel.cancelled().await;

    Ok(())
}

/// debug mode - talk to ourselves
pub async fn run_proxy_debug() -> anyhow::Result<()> {
    let cancel = tower_server::signal::termination_signal();
    let state = Arc::new(ProxlyState {
        ingress_fixed_dst_addr: Some("127.0.0.1:8088".parse()?),
        ingress_tls_config: None,
        cancel: cancel.clone(),
    });

    let ingress_listener = TcpListener::bind("0.0.0.0:4645").await?;

    tokio::spawn(ingress::ingress_proxy_tcp_listener_task(
        ingress_listener,
        state,
    ));

    let test_server = tower_server::Builder::new("0.0.0.0:8088".parse()?)
        .with_graceful_shutdown(cancel.clone())
        .with_scheme(tower_server::Scheme::Http)
        .bind()
        .await?;

    fn hello_router() -> axum::Router {
        use axum::routing::get;
        axum::Router::new().route(
            "/hello",
            get(|| async move {
                tracing::info!("answering hello");
                "hello"
            }),
        )
    }

    tokio::spawn(test_server.serve(hello_router()));

    cancel.cancelled().await;

    Ok(())
}

pub async fn provision() -> anyhow::Result<()> {
    Ok(())
}
