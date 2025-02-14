use std::sync::Arc;

use arc_swap::ArcSwap;
use hickory_resolver::{config::LookupIpStrategy, TokioAsyncResolver};
use rustls::{ClientConfig, ServerConfig};
use tokio_util::sync::CancellationToken;

pub struct ProxlyState {
    pub ingress_tls_config: Arc<ArcSwap<ServerConfig>>,
    pub egress_tls_config: Arc<ArcSwap<ClientConfig>>,
    pub hickory: hickory_resolver::TokioAsyncResolver,
    pub cancel: CancellationToken,
}

pub fn new_hickory() -> anyhow::Result<TokioAsyncResolver> {
    let (config, mut opts) = hickory_resolver::system_conf::read_system_conf()?;
    opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    Ok(TokioAsyncResolver::tokio(config, opts))
}
