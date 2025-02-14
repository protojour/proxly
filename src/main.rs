use clap::Parser;
use mimalloc::MiMalloc;
use tracing::info;
use tracing_subscriber::EnvFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(version, about, arg_required_else_help(true))]
struct Cli {}

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter(EnvFilter::from_env("PROXLY_LOG"))
        .init();

    info!("🦺 Proxly v{VERSION}");
    proxly::run_proxy().await?;

    Ok(())
}
