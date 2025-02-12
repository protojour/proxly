use std::sync::Arc;

use anyhow::Context;
use arc_swap::ArcSwap;
use futures_util::StreamExt;
use rustls::ServerConfig;
use tokio_util::sync::CancellationToken;

pub async fn authly_tls_config(
    authly_client: &authly_client::Client,
    cancel: CancellationToken,
) -> anyhow::Result<Arc<ArcSwap<ServerConfig>>> {
    let mut cfg_stream = authly_client
        // FIXME: figure out proper alternative names? hostname + ServiceId?
        .rustls_server_configurer("service", vec!["proxied".to_string()])
        .await?;

    let server_config = cfg_stream
        .next()
        .await
        .context("no TLS server config from Authly")?;

    let swap = Arc::new(ArcSwap::new(server_config));

    tokio::spawn({
        let swap = swap.clone();

        async move {
            loop {
                tokio::select! {
                    next = cfg_stream.next() => {
                        if let Some(next) = next {
                            swap.store(next);
                        } else {
                            return;
                        }
                    }
                    _ = cancel.cancelled() => {
                        return;
                    }
                }
            }
        }
    });

    Ok(swap)
}
