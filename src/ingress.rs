use std::{
    net::{IpAddr, SocketAddr},
    os::fd::AsFd,
    sync::Arc,
};

use anyhow::Context;
use authly_common::mtls_server::MTLSMiddleware;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio_rustls::TlsAcceptor;
use tower_server::tls::TlsConnectionMiddleware;
use tracing::{debug, info, warn, Instrument};

use crate::{ip_util::get_original_destination, state::ProxlyState};

pub async fn ingress_proxy_tcp_listener_task(
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

        let Ok((tcp_stream, peer_addr)) = result else {
            info!("couldn't get client, ignoring ingress connection");
            continue;
        };

        tokio::spawn({
            let state = state.clone();
            async move {
                if let Err(err) = forward_tcp(tcp_stream, peer_addr, state).await {
                    warn!(?err, "proxy_tcp error");
                }
            }
            .in_current_span()
        });
    }
}

async fn forward_tcp(
    mut tcp_stream: TcpStream,
    peer_addr: SocketAddr,
    state: Arc<ProxlyState>,
) -> anyhow::Result<()> {
    debug!(?peer_addr, "accepted");

    let mut dst_addr = {
        // FIXME: this is probably not the way to detect V6 in this context
        let is_v6 = matches!(peer_addr.ip(), IpAddr::V6(_));

        get_original_destination(&tcp_stream.as_fd(), is_v6).context("original destination")?
    };

    let mut terminate_tls = false;

    if dst_addr.port() == 443 {
        terminate_tls = true;
        dst_addr.set_port(80);
    }

    let mut dst_socket = TcpSocket::new_v4()?
        .connect(dst_addr)
        .await
        .context("cannot open forwarding socket")?;

    if terminate_tls {
        let tls_acceptor = TlsAcceptor::from(state.ingress_tls_config.load_full());
        let mut tls_stream = tls_acceptor
            .accept(tcp_stream)
            .await
            .context("failed to perform tls handshake")?;

        let peer_service_entity = MTLSMiddleware
            .data(tls_stream.get_ref().1)
            .and_then(|data| data.peer_service_entity())
            .context("no authly peer")?;

        info!(
            ?peer_addr,
            ?peer_service_entity,
            ?dst_addr,
            "forwarding mTLS service connection"
        );

        tokio::io::copy_bidirectional(&mut tls_stream, &mut dst_socket).await?;
    } else {
        info!(?peer_addr, ?dst_addr, "forwarding verbatim");

        tokio::io::copy_bidirectional(&mut tcp_stream, &mut dst_socket).await?;
    }

    Ok(())
}
