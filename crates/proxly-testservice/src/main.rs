use std::time::Duration;

use anyhow::Context;
use axum::routing::get;
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;
use tower_server::Scheme;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about, arg_required_else_help(true))]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run behind proxy - use only http
    Proxied,

    /// Run proxyless, with authly-client and mtls, interact with the other, proxied service
    Unproxied,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter(EnvFilter::from_env("PROXLY_LOG"))
        .init();

    let _ = rustls::crypto::ring::default_provider().install_default();

    let cancel = tower_server::signal::termination_signal();
    match Cli::parse().command {
        Some(Command::Proxied) => {
            spawn_proxied(cancel.clone()).await?;
        }
        Some(Command::Unproxied) => {
            spawn_unproxied(cancel.clone()).await?;
        }
        None => {}
    }

    cancel.cancelled().await;

    Ok(())
}

async fn spawn_proxied(cancel: CancellationToken) -> anyhow::Result<()> {
    let http_server = tower_server::Builder::new("0.0.0.0:80".parse()?)
        .with_graceful_shutdown(cancel.clone())
        .with_scheme(Scheme::Http)
        .bind()
        .await?;

    let http_client = reqwest::Client::new();

    tokio::spawn(get_hello_loop(
        http_client,
        "http://ts-unproxied/hello",
        cancel,
    ));

    tokio::spawn(http_server.serve(hello_router()));

    Ok(())
}

async fn spawn_unproxied(cancel: CancellationToken) -> anyhow::Result<()> {
    let authly_client = authly_client::Client::builder()
        .from_environment()
        .await?
        .connect()
        .await?;

    let http_server = tower_server::Builder::new("0.0.0.0:443".parse()?)
        .with_graceful_shutdown(cancel.clone())
        .with_scheme(Scheme::Https)
        .with_tls_connection_middleware(authly_common::mtls_server::MTLSMiddleware)
        .with_tls_config(
            authly_client
                .rustls_server_configurer(
                    "testservice unproxied!",
                    vec!["ts-unproxied".to_string()],
                )
                .await?,
        )
        .bind()
        .await?;

    let http_client = authly_client
        .request_client_builder_stream()?
        .next()
        .await
        .context("no HTTP client builder")?
        .build()?;

    tokio::spawn(get_hello_loop(
        http_client,
        "https://ts-proxied/hello",
        cancel,
    ));

    tokio::spawn(http_server.serve(hello_router()));

    Ok(())
}

fn hello_router() -> axum::Router {
    axum::Router::new().route(
        "/hello",
        get(|| async move {
            info!("answering hello");
            "hello"
        }),
    )
}

async fn get_hello_loop(
    http_client: reqwest::Client,
    url: &'static str,
    cancel: CancellationToken,
) {
    async fn get_hello(http_client: &reqwest::Client, url: &str) -> anyhow::Result<()> {
        info!("getting {url}");
        let hello = http_client.get(url).send().await?.text().await?;
        info!("got answer: `{hello}`");

        Ok(())
    }

    loop {
        if let Err(err) = get_hello(&http_client, &url).await {
            error!(?err, "GET {url}");
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = cancel.cancelled() => {
                return;
            }
        };
    }
}
