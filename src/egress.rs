use std::{
    net::{IpAddr, SocketAddr},
    os::fd::AsFd,
    sync::Arc,
};

use anyhow::Context;
use rustls::pki_types::ServerName;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio_rustls::TlsConnector;
use tracing::{info, info_span, warn, Instrument};

use crate::{ip_util::get_original_destination, state::ProxlyState};

pub async fn egress_proxy_tcp_listener_task(
    listener: TcpListener,
    state: Arc<ProxlyState>,
) -> anyhow::Result<()> {
    loop {
        let result = tokio::select! {
            result = listener.accept() => result,
            _ = state.cancel.cancelled() => {
                return Ok(());
            }
        };

        let Ok((tcp_stream, remote_addr)) = result else {
            info!("couldn't get client, ignoring ingress connection");
            continue;
        };

        let Ok(local_addr) = tcp_stream.local_addr() else {
            continue;
        };

        // FIXME: this is probably not the way to detect V6 in this context
        let is_v6 = matches!(remote_addr.ip(), IpAddr::V6(_));

        let Ok(orig_dst) = get_original_destination(&tcp_stream.as_fd(), is_v6) else {
            continue;
        };

        info!(
            "egress connection from {remote_addr:?}, local={local_addr:?}, orig_dst={orig_dst:?}"
        );

        tokio::spawn({
            let state = state.clone();
            async move {
                if let Err(err) = forward_tcp(tcp_stream, remote_addr, state).await {
                    warn!(?err, "proxy_tcp error");
                }
            }
            .instrument(info_span!("egress"))
        });
    }
}

async fn forward_tcp(
    mut tcp_stream: TcpStream,
    remote_addr: SocketAddr,
    state: Arc<ProxlyState>,
) -> anyhow::Result<()> {
    info!(?remote_addr, "accepted");

    let mut dst_addr = if let Some(addr) = state.ingress_fixed_dst_addr {
        addr
    } else {
        // FIXME: this is probably not the way to detect V6 in this context
        let is_v6 = matches!(remote_addr.ip(), IpAddr::V6(_));

        let result = get_original_destination(&tcp_stream.as_fd(), is_v6);
        tracing::info!(?result, "original destination");
        let addr = result?;

        addr
    };

    let mut tls_wrap = false;

    // port rerouting
    if dst_addr.port() == 80 {
        dst_addr.set_port(443);
        tls_wrap = true;
    }

    let mut dst_socket = TcpSocket::new_v4()?
        .connect(dst_addr)
        .await
        .context("cannot open forwarding socket")?;

    match state.egress_tls_config.as_ref() {
        Some(tls_config) if tls_wrap => {
            let tls_connector = TlsConnector::from(tls_config.load_full());
            let mut tls_stream = tls_connector
                .connect(
                    // FIXME: figure out service domain name
                    ServerName::DnsName("ts-unproxied".try_into().unwrap()),
                    dst_socket,
                )
                .await?;

            tokio::io::copy_bidirectional(&mut tcp_stream, &mut tls_stream).await?;
            Ok(())
        }
        Some(_) | None => {
            tokio::io::copy_bidirectional(&mut tcp_stream, &mut dst_socket).await?;
            Ok(())
        }
    }
}
