use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
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

    #[arg(long, value_enum, default_value = "json", global = true)]
    log_format: LogFormat,
}

#[derive(ValueEnum, Clone, Debug)]
enum LogFormat {
    Json,
    Pretty,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Запустить сервер (по умолчанию).
    Run,
    /// Проверить конфиг без запуска.
    CheckConfig,
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
        Some(Command::CheckConfig) => skadi_server::check_config(&cli.config),
        Some(Command::Genkey { kind }) => match kind {
            GenkeyKind::Reality => skadi_server::genkey::generate_reality_keys(),
        },
        Some(Command::Run) | None => {
            init_tracing(&cli)?;
            run_server(cli).await
        }
    }
}

fn init_tracing(cli: &Cli) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| cli.log_level.clone().into());

    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match cli.log_format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Pretty => builder.init(),
    }
    Ok(())
}

async fn run_server(cli: Cli) -> Result<()> {
    let config = Config::load(&cli.config)
        .with_context(|| format!("failed to load config from {:?}", cli.config))?;

    info!(
        listen = %config.server.listen,
        tls = config.tls_enabled(),
        reality = config.reality_enabled(),
        api = config.api.enabled,
        metrics = config.metrics.enabled,
        "SkadiCore starting"
    );

    skadi_server::run(config, cli.config).await
}
