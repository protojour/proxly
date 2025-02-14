use std::{
    net::{IpAddr, SocketAddr},
    os::fd::AsFd,
    sync::Arc,
};

use anyhow::Context;
use rustls::pki_types::ServerName;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn, Instrument};

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

        let Ok((tcp_stream, peer_addr)) = result else {
            warn!("couldn't get client, ignoring connection");
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

    let apply_tls = analyze_destination(&mut dst_addr, &state).await;

    let mut dst_socket = TcpSocket::new_v4()?
        .connect(dst_addr)
        .await
        .context("cannot open forwarding socket")?;

    if let Some(ApplyAuthlyMTLS { server_name }) = apply_tls {
        info!(
            ?server_name,
            ?dst_addr,
            ?peer_addr,
            "TLS-wrapping connection"
        );

        let mut tls_stream = TlsConnector::from(state.egress_tls_config.load_full())
            .connect(server_name, dst_socket)
            .await?;

        tokio::io::copy_bidirectional(&mut tcp_stream, &mut tls_stream).await?;
    } else {
        tokio::io::copy_bidirectional(&mut tcp_stream, &mut dst_socket).await?;
    }

    Ok(())
}

struct ApplyAuthlyMTLS {
    server_name: ServerName<'static>,
}

async fn analyze_destination(
    dst_addr: &mut SocketAddr,
    state: &ProxlyState,
) -> Option<ApplyAuthlyMTLS> {
    let mut should_tls_wrap = false;

    let is_private = match dst_addr.ip() {
        IpAddr::V4(v4) => v4.is_private(),
        IpAddr::V6(_v6) => {
            warn!("IPv6 private range detection not implemented");
            false
        }
    };

    // for now, only apply mTLS-wrapping if the address is private (cluster-local)
    // and the outbound port is 80, i.e. insecure HTTP
    if is_private && dst_addr.port() == 80 {
        // change the port to 443 to talk "proper" https
        dst_addr.set_port(443);
        should_tls_wrap = true;
    }

    if should_tls_wrap {
        match find_tls_server_name(dst_addr.ip(), &state).await {
            Ok(server_name) => Some(ApplyAuthlyMTLS { server_name }),
            Err(err) => {
                warn!(?err, "failed to find tls server name");
                None
            }
        }
    } else {
        None
    }
}

async fn find_tls_server_name(
    addr: IpAddr,
    state: &ProxlyState,
) -> anyhow::Result<ServerName<'static>> {
    let reverse_lookup = state.hickory.reverse_lookup(addr).await?;
    let dns_ptr = reverse_lookup.into_iter().next().context("no DNS name")?;
    let mut dns_name = dns_ptr.to_ascii();

    if dns_name.ends_with('.') {
        dns_name.pop();
    }

    Ok(ServerName::DnsName(dns_name.try_into()?))
}
