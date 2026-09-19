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
    /// Локальный SOCKS5 → удалённый VLESS+TLS (клиентский режим).
    Client,
    /// Проверить конфиг без запуска.
    CheckConfig,
    /// Сгенерировать ключи (REALITY или AmneziaWG).
    Genkey {
        #[arg(value_enum, default_value = "reality")]
        kind: GenkeyKind,
    },
    /// Экспорт клиентского AWG `.conf` из server.toml.
    ExportAwgClient {
        /// Индекс peer в `[[transport.awg.peers]]` (0-based).
        #[arg(long, default_value = "0")]
        peer: usize,
        /// Приватный ключ клиента (WireGuard base64).
        #[arg(long)]
        client_key: String,
        /// Endpoint сервера `host:port`.
        #[arg(long)]
        endpoint: String,
        /// Путь для записи `client.conf`.
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum GenkeyKind {
    Reality,
    Awg,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::CheckConfig) => skadi_server::check_config(&cli.config),
        Some(Command::Genkey { kind }) => match kind {
            GenkeyKind::Reality => skadi_server::genkey::generate_reality_keys(),
            GenkeyKind::Awg => skadi_server::genkey::generate_awg_keys(),
        },
        Some(Command::ExportAwgClient {
            peer,
            client_key,
            endpoint,
            output,
        }) => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load config from {:?}", cli.config))?;
            let conf = config.export_awg_client_conf(peer, &client_key, &endpoint)?;
            std::fs::write(&output, conf)
                .with_context(|| format!("failed to write {:?}", output))?;
            println!("AWG client config written to {}", output.display());
            Ok(())
        }
        Some(Command::Client) => {
            init_tracing(&cli)?;
            run_client(cli).await
        }
        Some(Command::Run) | None => {
            init_tracing(&cli)?;
            run_server(cli).await
        }
    }
}

fn init_tracing(cli: &Cli) -> Result<()> {
    use skadi_server::observability::tracing_init::{self, LogFormat as TracingLogFormat};

    let format = match cli.log_format {
        LogFormat::Json => TracingLogFormat::Json,
        LogFormat::Pretty => TracingLogFormat::Pretty,
    };
    tracing_init::init_tracing(&cli.log_level, format)
}

async fn run_client(cli: Cli) -> Result<()> {
    use skadi_client::ClientConfig;
    use tokio::sync::watch;

    let config = ClientConfig::load(&cli.config)
        .with_context(|| format!("failed to load client config from {:?}", cli.config))?;

    info!(
        socks5 = ?config.client.listen,
        tun = config.client.tun.enabled,
        awg = config.awg_enabled(),
        remote = ?config.remote.as_ref().map(|r| r.server.as_str()),
        "SkadiCore client starting"
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = shutdown_tx.send(true);
        }
    });

    skadi_client::run(config, shutdown_rx).await?;
    shutdown_task.abort();
    Ok(())
}

async fn run_server(cli: Cli) -> Result<()> {
    let config = Config::load(&cli.config)
        .with_context(|| format!("failed to load config from {:?}", cli.config))?;

    info!(
        listen = %config.server.listen,
        tls = config.tls_enabled(),
        reality = config.reality_enabled(),
        awg = config.awg_enabled(),
        hysteria2 = config.hysteria2_enabled(),
        tuic = config.tuic_enabled(),
        api = config.api.enabled,
        metrics = config.metrics.enabled,
        "SkadiCore starting"
    );

    skadi_server::run(config, cli.config).await
}
