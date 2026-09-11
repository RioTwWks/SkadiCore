use anyhow::{Context, Result};
use clap::Parser;
use skadi_server::config::Config;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "skadicore", version, about = "SkadiCore proxy kernel")]
struct Cli {
    #[arg(short, long, default_value = "config/skadi.toml")]
    config: PathBuf,

    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .json()
        .init();

    let config = Config::load(&cli.config)
        .with_context(|| format!("failed to load config from {:?}", cli.config))?;

    info!(
        listen = %config.server.listen,
        tls = config.tls_enabled(),
        "SkadiCore starting"
    );

    skadi_server::run(config).await
}
