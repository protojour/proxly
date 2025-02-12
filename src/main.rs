use clap::{Parser, Subcommand};
use mimalloc::MiMalloc;
use proxly::{run_proxy, run_proxy_debug};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about, arg_required_else_help(true))]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run Proxly in proxy server mode (default)
    Proxy,

    /// Run Proxly in proxy debug mode
    ProxyDebug,
}

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter(EnvFilter::from_env("PROXLY_LOG"))
        .init();

    match Cli::parse().command {
        Some(Command::Proxy) | None => {
            info!(
                "starting proxly proxy server, running as uid={}",
                users::get_current_uid()
            );

            run_proxy().await?;
        }
        Some(Command::ProxyDebug) => {
            info!("starting proxly proxy server (debug mode)");
            run_proxy_debug().await?;
        }
    }

    Ok(())
}
