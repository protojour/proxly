use std::sync::Arc;

use arc_swap::ArcSwap;
use hickory_resolver::{config::LookupIpStrategy, TokioResolver};
use rustls::{ClientConfig, ServerConfig};
use tokio_util::sync::CancellationToken;

pub struct ProxlyState {
    pub ingress_tls_config: Arc<ArcSwap<ServerConfig>>,
    pub egress_tls_config: Arc<ArcSwap<ClientConfig>>,
    pub hickory: hickory_resolver::TokioResolver,
    pub cancel: CancellationToken,
}

pub fn new_hickory() -> anyhow::Result<TokioResolver> {
    let mut builder = TokioResolver::builder_tokio()?;
    builder.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    Ok(builder.build())
}
