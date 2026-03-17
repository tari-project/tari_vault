# IMP-009: Migrate from `log4rs` to `tracing`

**Status:** `[ ]` Planned
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

## Proposed Changes

### Dependencies

```toml
# Remove:
log4rs = "1.4"

# Add:
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
```

The existing `log` facade calls (`log::info!`, `log::warn!`, `log::error!`) are compatible with `tracing` via the `tracing-log` bridge, allowing incremental migration.

### Initialization

Replace `log4rs::init_file(...)` in `src/main.rs` with:

```rust
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

Or with JSON output for structured log ingestion:

```rust
tracing_subscriber::fmt()
    .json()
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

### Instrumentation Opportunities

Once on `tracing`, key async operations can be instrumented with spans:

```rust
#[tracing::instrument(skip(self, proof), fields(record_id))]
async fn store_proof(...) { ... }
```

This provides request-scoped context in logs without manual field threading.

### Configuration

`RUST_LOG=tari_vault=info` replaces the YAML log config. The `--debug` CLI flag can set `RUST_LOG=tari_vault=debug` programmatically.

## Affected Files

- `src/main.rs` — init change, remove YAML log config path
- `src/config.rs` — remove `log_config_path` if present
- All `log::` call sites — can migrate incrementally to `tracing::` macros
- `Cargo.toml`
- Any `log4rs.yaml` / `log4rs-debug.yaml` config files

## Notes

- This is the most involved change in Tier 4 but yields the highest operational value.
- The `log` → `tracing` migration can be done incrementally: add `tracing-log` bridge first, then replace macros file by file.
- Consider enabling `tokio-console` support via `console-subscriber` as a follow-on once `tracing` is in place.
