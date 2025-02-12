use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    os::fd::AsFd,
    sync::Arc,
};

use anyhow::Context;
use authly_common::mtls_server::MTLSMiddleware;
use nix::sys::socket::sockopt::{Ip6tOriginalDst, OriginalDst};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio_rustls::TlsAcceptor;
use tower_server::tls::TlsConnectionMiddleware;
use tracing::{info, warn};

use crate::state::ProxlyState;

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

        let Ok((tcp_stream, remote_addr)) = result else {
            info!("couldn't get client, ignoring ingress connection");
            continue;
        };

        tokio::spawn({
            let state = state.clone();
            async move {
                if let Err(err) = forward_tcp(tcp_stream, remote_addr, state).await {
                    warn!(?err, "proxy_tcp error");
                }
            }
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

    // port rerouting
    if dst_addr.port() == 443 {
        dst_addr.set_port(80);
    }

    let mut dst_socket = TcpSocket::new_v4()?
        .connect(dst_addr)
        .await
        .context("cannot open forwarding socket")?;

    match state.ingress_tls_config.as_ref() {
        Some(tls_config) => {
            let tls_acceptor = TlsAcceptor::from(tls_config.load_full());
            let mut tls_stream = tls_acceptor
                .accept(tcp_stream)
                .await
                .context("failed to perform tls handshake")?;

            {
                let peer_service_entity = MTLSMiddleware
                    .data(tls_stream.get_ref().1)
                    .and_then(|data| data.peer_service_entity())
                    .context("no authly peer")?;

                info!("got connection from {}", peer_service_entity);
            }

            tokio::io::copy_bidirectional(&mut tls_stream, &mut dst_socket).await?;
            Ok(())
        }
        None => {
            tokio::io::copy_bidirectional(&mut tcp_stream, &mut dst_socket).await?;
            Ok(())
        }
    }
}

fn get_original_destination(fd: &impl AsFd, is_v6: bool) -> anyhow::Result<SocketAddr> {
    if is_v6 {
        let in_addr = nix::sys::socket::getsockopt(fd, Ip6tOriginalDst)?;
        let addr = in_addr.sin6_addr.s6_addr;

        Ok(SocketAddr::V6(SocketAddrV6::new(
            Ipv6Addr::new(
                (addr[0] as u16) << 8 | (addr[1] as u16),
                (addr[2] as u16) << 8 | (addr[3] as u16),
                (addr[4] as u16) << 8 | (addr[5] as u16),
                (addr[6] as u16) << 8 | (addr[7] as u16),
                (addr[8] as u16) << 8 | (addr[9] as u16),
                (addr[10] as u16) << 8 | (addr[11] as u16),
                (addr[12] as u16) << 8 | (addr[13] as u16),
                (addr[14] as u16) << 8 | (addr[15] as u16),
            ),
            u16::from_be(in_addr.sin6_port),
            in_addr.sin6_flowinfo,
            in_addr.sin6_scope_id,
        )))
    } else {
        let in_addr = nix::sys::socket::getsockopt(fd, OriginalDst)?;
        let addr = u32::from_be(in_addr.sin_addr.s_addr);

        Ok(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(
                (addr >> 24) as u8,
                (addr >> 16) as u8,
                (addr >> 8) as u8,
                addr as u8,
            ),
            u16::from_be(in_addr.sin_port),
        )))
    }
}
