use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use clap::Parser;
use tokio_util::sync::CancellationToken;

use tari_vault::{
    config::{VaultConfig, load_config},
    rpc::start_server,
    storage::LocalFileStore,
    vault::{ProofVault, StandardVault, spawn_cleanup_task},
};

/// Tari Secure Proof Handoff Vault
///
/// Stores encrypted L1 Merkle Proofs and hands them off to L2 wallets via
/// a JSON-RPC 2.0 HTTP interface without ever exposing the plaintext proof
/// to intermediaries.
#[derive(Parser, Debug)]
#[command(name = "tari_vault", version, about, long_about = None)]
struct Cli {
    /// Path to a TOML or YAML configuration file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Override the vault storage file path.
    #[arg(long, value_name = "FILE")]
    vault_file: Option<PathBuf>,

    /// Override the server bind address (e.g. 127.0.0.1:9000).
    #[arg(long, value_name = "ADDR")]
    bind: Option<String>,

    /// Override the expired-proof cleanup interval in seconds (0 = disabled).
    #[arg(long, value_name = "SECS")]
    cleanup_interval: Option<u64>,

    /// Path to a log4rs YAML configuration file.
    #[arg(long, value_name = "FILE")]
    log_config: Option<PathBuf>,

    /// Log level used when no log config file is provided.
    /// One of: error, warn, info, debug, trace.
    #[arg(long, default_value = "info")]
    log_level: Option<String>,

    /// Bearer token required in the `Authorization` header on every RPC
    /// request.  Omit (or leave empty) to disable authentication.
    #[arg(long, value_name = "TOKEN")]
    auth_token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut cfg = load_config(cli.config.as_deref())?;

    // CLI flags take the highest priority.
    if let Some(vault_file) = cli.vault_file {
        cfg.storage.vault_file = vault_file;
    }
    if let Some(bind) = cli.bind {
        cfg.server.bind_address = bind;
    }
    if let Some(secs) = cli.cleanup_interval {
        cfg.storage.cleanup_interval_secs = secs;
    }
    if let Some(level) = cli.log_level {
        cfg.logging.level = level;
    }
    if let Some(log_cfg) = cli.log_config {
        cfg.logging.config_file = Some(log_cfg);
    }
    if let Some(token) = cli.auth_token {
        cfg.server.auth_token = Some(token);
    }

    init_logging(&cfg)?;

    log::info!(target: "tari_vault", "Starting Tari Vault v{}", env!("CARGO_PKG_VERSION"));
    log::debug!(target: "tari_vault", "Config: {:?}", cfg);

    let storage = LocalFileStore::new(cfg.storage.vault_file.clone())
        .context("Failed to open vault storage file")?;
    let vault = Arc::new(StandardVault::new(storage));

    let purged = vault.cleanup().await.context("Startup cleanup failed")?;
    if purged > 0 {
        log::info!(target: "tari_vault", "Startup cleanup: removed {purged} expired proof(s)");
    }

    let shutdown = CancellationToken::new();
    let cleanup_task = if cfg.storage.cleanup_interval_secs > 0 {
        let interval = Duration::from_secs(cfg.storage.cleanup_interval_secs);
        log::info!(
            target: "tari_vault",
            "Background cleanup enabled (interval: {}s)", interval.as_secs()
        );
        Some(spawn_cleanup_task(
            Arc::clone(&vault),
            interval,
            shutdown.clone(),
        ))
    } else {
        log::info!(target: "tari_vault", "Background cleanup disabled (cleanup_interval_secs = 0)");
        None
    };

    let (_addr, server_handle) = start_server(
        &cfg.server.bind_address,
        Arc::clone(&vault),
        cfg.server.auth_token.clone(),
    )
    .await
    .context("Failed to start RPC server")?;

    tokio::signal::ctrl_c()
        .await
        .context("Failed to listen for Ctrl-C")?;

    log::info!(target: "tari_vault", "Shutdown signal received");

    // Stop the RPC server first (no new requests accepted).
    server_handle.stop()?;
    server_handle.stopped().await;
    log::info!(target: "tari_vault", "RPC server stopped");

    // Cancel the cleanup task and wait for it to exit.
    shutdown.cancel();
    if let Some(task) = cleanup_task {
        task.stopped().await;
        log::info!(target: "tari_vault", "Cleanup task stopped");
    }

    log::info!(target: "tari_vault", "Shutdown complete");
    Ok(())
}

fn init_logging(cfg: &VaultConfig) -> anyhow::Result<()> {
    if let Some(log_cfg_path) = &cfg.logging.config_file {
        log4rs::init_file(log_cfg_path, Default::default())
            .with_context(|| format!("Failed to load log config from {log_cfg_path:?}"))?;
        return Ok(());
    }

    use log::LevelFilter;
    use log4rs::{
        append::console::ConsoleAppender,
        config::{Appender, Config, Root},
        encode::pattern::PatternEncoder,
    };

    let level: LevelFilter = cfg.logging.level.parse().unwrap_or(LevelFilter::Info);

    let console = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%dT%H:%M:%S%.3fZ)(utc)} {h({l:<5})} {t} — {m}{n}",
        )))
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("console", Box::new(console)))
        .build(Root::builder().appender("console").build(level))
        .context("Failed to build log4rs config")?;

    log4rs::init_config(config).context("Failed to initialise log4rs")?;

    Ok(())
}
