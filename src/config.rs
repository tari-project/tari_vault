use std::path::{Path, PathBuf};

use anyhow::Context;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};

/// Top-level vault configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub logging: LoggingConfig,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// TCP address the JSON-RPC HTTP server binds to.
    pub bind_address: String,

    /// Optional Bearer token required in the `Authorization: Bearer <TOKEN>`
    /// header on every JSON-RPC request.
    ///
    /// `None` (or an empty string) disables authentication — suitable for
    /// local development or when the caller handles auth at a higher layer.
    ///
    /// Set via `VAULT__SERVER__AUTH_TOKEN` environment variable or the
    /// `server.auth_token` key in the config file.
    pub auth_token: Option<String>,

    /// Path to the TLS certificate file (PEM-encoded).
    ///
    /// Both `tls_cert_path` and `tls_key_path` must be set to enable TLS.
    /// TLS is required when binding to a non-loopback address.
    ///
    /// Set via `VAULT__SERVER__TLS_CERT_PATH` or `server.tls_cert_path` in
    /// the config file.
    pub tls_cert_path: Option<PathBuf>,

    /// Path to the TLS private key file (PEM-encoded, PKCS#8 or PKCS#1 RSA).
    ///
    /// Set via `VAULT__SERVER__TLS_KEY_PATH` or `server.tls_key_path` in
    /// the config file.
    pub tls_key_path: Option<PathBuf>,

    /// Allow plain HTTP on a non-loopback address.
    ///
    /// **Security risk.** Only enable this when TLS is terminated externally
    /// (e.g. an nginx/Envoy sidecar or a k8s Ingress controller) and the
    /// vault is not reachable outside the trusted network.
    ///
    /// Set via `VAULT__SERVER__INSECURE_NO_TLS=true` or
    /// `server.insecure_no_tls = true` in the config file.
    #[serde(default)]
    pub insecure_no_tls: bool,
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("bind_address", &self.bind_address)
            .field(
                "auth_token",
                &self.auth_token.as_deref().map(|_| "<redacted>"),
            )
            .field("tls_cert_path", &self.tls_cert_path)
            .field("tls_key_path", &self.tls_key_path)
            .field("insecure_no_tls", &self.insecure_no_tls)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the JSON vault file.
    pub vault_file: PathBuf,

    /// How often the background cleanup task sweeps for expired proofs, in
    /// seconds.  Set to `0` to disable the automatic sweep (you can still
    /// call `ProofVault::cleanup()` manually).
    ///
    /// Default: 300 (5 minutes).
    pub cleanup_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Optional path to a log4rs YAML config file.
    pub config_file: Option<PathBuf>,
    /// Fallback log level used when `config_file` is absent.
    pub level: String,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_address: "127.0.0.1:9000".to_string(),
                auth_token: None,
                tls_cert_path: None,
                tls_key_path: None,
                insecure_no_tls: false,
            },
            storage: StorageConfig {
                vault_file: default_vault_path(),
                cleanup_interval_secs: 300,
            },
            logging: LoggingConfig {
                config_file: None,
                level: "info".to_string(),
            },
        }
    }
}

/// Load configuration by layering sources in priority order (low → high):
///
/// 1. Built-in defaults.
/// 2. Config file (`vault_config.toml` in the working directory, or the path
///    provided by `config_file`).
/// 3. Environment variables (`VAULT__SERVER__BIND_ADDRESS`, etc.).
///
/// Loads a `.env` file from the working directory if present.
pub fn load_config(config_file: Option<&Path>) -> anyhow::Result<VaultConfig> {
    // Best-effort — silently ignore missing .env file.
    dotenv::dotenv().ok();

    let defaults = VaultConfig::default();

    let mut builder = Config::builder()
        .set_default("server.bind_address", defaults.server.bind_address)?
        .set_default(
            "storage.vault_file",
            defaults.storage.vault_file.to_string_lossy().as_ref(),
        )?
        .set_default(
            "storage.cleanup_interval_secs",
            defaults.storage.cleanup_interval_secs,
        )?
        .set_default("logging.level", defaults.logging.level)?;

    // Config file source.
    if let Some(path) = config_file {
        builder = builder.add_source(File::from(path).required(true));
    } else {
        // Optional auto-discovery — supports `.toml`, `.yaml`, `.json`, etc.
        builder = builder.add_source(File::with_name("vault_config").required(false));
    }

    // Environment variables override everything else.
    builder = builder.add_source(
        Environment::with_prefix("VAULT")
            .separator("__")
            .try_parsing(true),
    );

    builder
        .build()
        .context("Failed to build configuration")?
        .try_deserialize()
        .context("Failed to deserialise configuration")
}

fn default_vault_path() -> PathBuf {
    // Prefer XDG / platform data dir; fall back to current directory.
    let base = dirs_next::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("tari_vault").join("vault.json")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let cfg = VaultConfig::default();
        assert_eq!(cfg.server.bind_address, "127.0.0.1:9000");
        assert!(cfg.storage.vault_file.ends_with("vault.json"));
        assert_eq!(cfg.logging.level, "info");
    }

    #[test]
    fn load_with_no_files_uses_defaults() {
        // Only assert the default if the override env var is not set.
        if std::env::var("VAULT__SERVER__BIND_ADDRESS").is_err() {
            let cfg = load_config(None).unwrap();
            assert_eq!(cfg.server.bind_address, "127.0.0.1:9000");
        }
    }
}
