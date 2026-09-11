use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use skadi_server::config::Config;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "skadicore", version, about = "SkadiCore proxy kernel")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long, default_value = "config/skadi.toml", global = true)]
    config: PathBuf,

    #[arg(long, default_value = "info", global = true)]
    log_level: String,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Запустить сервер (по умолчанию).
    Run,
    /// Сгенерировать ключи REALITY.
    Genkey {
        #[arg(value_enum, default_value = "reality")]
        kind: GenkeyKind,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum GenkeyKind {
    Reality,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Genkey { kind }) => match kind {
            GenkeyKind::Reality => skadi_server::genkey::generate_reality_keys(),
        },
        Some(Command::Run) | None => run_server(cli).await,
    }
}

async fn run_server(cli: Cli) -> Result<()> {
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
        reality = config.reality_enabled(),
        api = config.api.enabled,
        "SkadiCore starting"
    );

    skadi_server::run(config).await
}
