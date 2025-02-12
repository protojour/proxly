use std::{net::SocketAddr, sync::Arc};

use arc_swap::ArcSwap;
use rustls::{ClientConfig, ServerConfig};
use tokio_util::sync::CancellationToken;

pub struct ProxlyState {
    pub ingress_fixed_dst_addr: Option<SocketAddr>,
    pub ingress_tls_config: Option<Arc<ArcSwap<ServerConfig>>>,
    pub egress_tls_config: Option<Arc<ArcSwap<ClientConfig>>>,
    pub cancel: CancellationToken,
}
