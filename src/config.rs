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

    /// Maximum allowed serialised size of `proof_json` in bytes.
    ///
    /// Requests whose serialised `proof_json` exceeds this limit are rejected
    /// with an `InvalidParameter` error before any encryption or storage
    /// occurs.  The HTTP transport enforces the same cap on the full request
    /// body to stop amplification at the transport layer.
    ///
    /// Default: 1 MiB (1 048 576 bytes).
    ///
    /// Set via `VAULT__SERVER__MAX_PROOF_SIZE_BYTES` or
    /// `server.max_proof_size_bytes` in the config file.
    #[serde(default = "default_max_proof_size_bytes")]
    pub max_proof_size_bytes: usize,
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
            .field("max_proof_size_bytes", &self.max_proof_size_bytes)
            .finish()
    }
}

/// Which storage backend to use.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// JSON file backend (default).  Zero external dependencies.
    #[default]
    File,
    /// SQLite backend.  O(1) per operation, atomic fetch+delete.
    Sqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the JSON vault file (used when `backend = "file"`).
    pub vault_file: PathBuf,

    /// How often the background cleanup task sweeps for expired proofs, in
    /// seconds.  Set to `0` to disable the automatic sweep (you can still
    /// call `ProofVault::cleanup()` manually).
    ///
    /// Default: 300 (5 minutes).
    pub cleanup_interval_secs: u64,

    /// Which storage backend to use.  Default: `file`.
    ///
    /// Set via `VAULT__STORAGE__BACKEND=sqlite` or `storage.backend = "sqlite"`
    /// in the config file.
    #[serde(default)]
    pub backend: BackendKind,

    /// Path to the SQLite database file (used when `backend = "sqlite"`).
    ///
    /// Defaults to the same directory as `vault_file`, named `vault.db`.
    ///
    /// Set via `VAULT__STORAGE__SQLITE_PATH` or `storage.sqlite_path` in the
    /// config file.
    #[serde(default)]
    pub sqlite_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level used when `RUST_LOG` is not set.
    /// One of: error, warn, info, debug, trace.
    pub level: String,
}

fn default_max_proof_size_bytes() -> usize {
    1_048_576 // 1 MiB
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
                max_proof_size_bytes: default_max_proof_size_bytes(),
            },
            storage: StorageConfig {
                vault_file: default_vault_path(),
                cleanup_interval_secs: 300,
                backend: BackendKind::File,
                sqlite_path: None,
            },
            logging: LoggingConfig {
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
    dotenvy::dotenv().ok();

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
        .set_default("storage.backend", "file")?
        .set_default("logging.level", defaults.logging.level)?
        .set_default(
            "server.max_proof_size_bytes",
            defaults.server.max_proof_size_bytes as u64,
        )?;

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
