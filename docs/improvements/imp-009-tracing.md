# IMP-009: Migrate from `log4rs` to `tracing`

**Status:** `[x]` Completed
**Tier:** 4 — Dependency Hygiene
**Priority:** Medium

## Problem

The project uses `log4rs` with a YAML configuration file for logging. While functional, `log4rs` is:

- Heavy: requires a YAML config file and a non-trivial setup path.
- Not async-aware: it treats all log events as point-in-time records with no context about which async task produced them.
- Misaligned with the tokio ecosystem: the standard tokio/async-Rust observability stack is built around `tracing`.

`tracing` provides structured, span-based diagnostics natively integrated with `tokio`. It supports `tokio-console` for live async task inspection, OpenTelemetry export, and is the default in all major async Rust frameworks (`axum`, `tonic`, `tower`).

## Goal

Replace `log4rs` with `tracing` + `tracing-subscriber` for a more idiomatic, operationally richer logging setup.

## What Was Done

### Dependencies (`Cargo.toml`)

Removed:
```toml
log = { version = "0.4.29", features = ["kv"] }
log4rs = { version = "1.4.0", features = ["yaml_format", "console_appender", "file_appender", "log_kv"] }
```

Added:
```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

All 29 `log::` call sites across 5 files were migrated directly to `tracing::` macros, so no `tracing-log` bridge was needed.

### Initialization (`src/main.rs`)

Replaced the two-path `init_logging()` (YAML file vs programmatic log4rs config) with:

```rust
fn init_logging(cfg: &VaultConfig) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cfg.logging.level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}
```

`RUST_LOG` takes priority. When absent, `--log-level` / `VAULT__LOGGING__LEVEL` (default: `info`) is used.

### Config (`src/config.rs`)

Removed `LoggingConfig.config_file: Option<PathBuf>` — no equivalent concept in `tracing-subscriber`.

### CLI (`src/main.rs`)

Removed `--log-config <FILE>` argument. The YAML config path is no longer supported.

### Files deleted

- `log4rs.yaml` — replaced by `RUST_LOG` environment variable

### Files changed

| File | Change |
|------|--------|
| `Cargo.toml` | Swapped deps |
| `src/config.rs` | Removed `config_file` field |
| `src/main.rs` | Rewrote `init_logging()`, removed `--log-config` arg, migrated log macros |
| `src/vault/proof_vault.rs` | `log::` → `tracing::` (4 statements) |
| `src/vault/cleanup.rs` | `log::` → `tracing::` (6 statements) |
| `src/rpc/server.rs` | `log::` → `tracing::` (7 statements) |
| `src/auth.rs` | `log::` → `tracing::` (1 statement) |

## Configuration

```bash
# Set level via env (takes priority)
RUST_LOG=tari_vault=debug ./tari_vault

# Fine-grained per-target control
RUST_LOG=tari_vault::vault=debug,tari_vault=info ./tari_vault

# Inspect jsonrpsee internal spans (now visible because tracing-subscriber captures them)
RUST_LOG=tari_vault=info,jsonrpsee=debug ./tari_vault

# Fallback level via flag (used when RUST_LOG is absent)
./tari_vault --log-level debug
```

## Follow-on Opportunity

`tokio-console` support can be added via `console-subscriber` — requires enabling `tokio_unstable` and adding the `console-subscriber` crate. Now that `tracing` is in place this is a one-file change.
