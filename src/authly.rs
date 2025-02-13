use std::sync::Arc;

use anyhow::Context;
use arc_swap::ArcSwap;
use futures_util::StreamExt;
use rustls::{
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
    ClientConfig, RootCertStore, ServerConfig,
};
use tokio_util::sync::CancellationToken;

pub async fn authly_tls_server_config(
    authly_client: &authly_client::Client,
    cancel: CancellationToken,
) -> anyhow::Result<Arc<ArcSwap<ServerConfig>>> {
    let mut cfg_stream = authly_client.rustls_server_configurer("proxly").await?;

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

pub async fn authly_tls_client_config(
    authly_client: &authly_client::Client,
    cancel: CancellationToken,
) -> anyhow::Result<Arc<ArcSwap<ClientConfig>>> {
    let mut params_stream = authly_client.connection_params_stream();

    let params = params_stream
        .next()
        .await
        .context("no connection params from Authly")?;

    fn client_config(
        params: &authly_client::connection::ConnectionParams,
    ) -> anyhow::Result<Arc<ClientConfig>> {
        let mut root_store = RootCertStore::empty();
        root_store.add(CertificateDer::from_pem_reader(params.ca_pem())?)?;

        let identity_cert = CertificateDer::from_pem_reader(params.identity().cert_pem().as_ref())?;
        let identity_key = PrivateKeyDer::from_pem_reader(params.identity().key_pem().as_ref())?;

        Ok(Arc::new(
            ClientConfig::builder()
                .with_root_certificates(Arc::new(root_store))
                .with_client_auth_cert(vec![identity_cert], identity_key)?,
        ))
    }

    let swap = Arc::new(ArcSwap::new(client_config(&params)?));

    tokio::spawn({
        let swap = swap.clone();

        async move {
            loop {
                tokio::select! {
                    next = params_stream.next() => {
                        if let Some(next) = next {
                            if let Ok(cfg) = client_config(&next) {
                                swap.store(cfg);
                            }
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
