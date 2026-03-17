# Development Guide

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.75+ | https://rustup.rs |
| cargo | (bundled) | |
| Python 3 | any | for `check_openrpc.sh` |
| Node.js + npx | optional | enables deep OpenRPC schema validation |

---

## Setup

```bash
git clone <repo>
cd tari_vault
cargo build
```

No additional setup steps. The vault file is created automatically on first run.

---

## Makefile

All common tasks are available via `make`. Run `make help` to list them.

```
make build           Compile the library and binary (debug)
make build-release   Compile with release optimisations
make test            Run all tests (unit + integration + doctests)
make test-unit       Run unit tests only (lib crate)
make test-integration Run integration tests only
make lint            Run Clippy (deny warnings)
make fmt             Format all source files in-place
make fmt-check       Check formatting without modifying files (CI)
make check           Full pre-commit check: format + lint + test
make run             Start the vault server with default settings
make run-debug       Start the vault server at debug log level
make check-openrpc   Validate openrpc.json structure and method coverage
make ci              Full CI pipeline: fmt-check + lint + test + check-openrpc
```

Run `make ci` before pushing to ensure the full pipeline is clean.

---

## Running Tests

```bash
# All tests
make test

# Only unit tests (fast, no network/server)
make test-unit

# Only integration tests (spins up real servers)
make test-integration

# Specific test by name
cargo test store_and_retrieve_via_rpc
```

### Test layout

| Location | Type | What it tests |
|----------|------|---------------|
| `src/auth.rs` `#[cfg(test)]` | Unit | Tower middleware: disabled, valid, missing, wrong token |
| `src/rpc/discovery.rs` `#[cfg(test)]` | Unit | `openrpc.json` validity and coverage |
| `src/vault/proof_vault.rs` `#[cfg(test)]` | Unit | Encrypt/decrypt, single-use, TTL, delete, wrong key |
| `src/vault/cleanup.rs` `#[cfg(test)]` | Unit | Background task: sweep, cancellation, parent token |
| `src/storage/local_file.rs` `#[cfg(test)]` | Unit | CRUD, expired record deletion |
| `src/domain/claim_id.rs` `#[cfg(test)]` | Unit | Round-trip, invalid base64, wrong length |
| `src/config.rs` `#[cfg(test)]` | Unit | Defaults, env var loading |
| `tests/rpc_integration.rs` | Integration | Full HTTP JSON-RPC calls: store, retrieve, delete, auth |

---

## Linting and Formatting

```bash
make fmt       # reformat in-place
make lint      # clippy -D warnings
make fmt-check # check only (CI-safe, no modifications)
```

Clippy runs with `-D warnings` — all warnings are treated as errors. Fix warnings before committing.

---

## Adding a New RPC Method

### 1. Add the method to the `VaultRpc` trait

In `src/rpc/api.rs`, add a new `#[method]` to the `VaultRpc` trait:

```rust
#[rpc(server, namespace = "vault")]
pub trait VaultRpc {
    // ... existing methods ...

    /// Brief description.
    #[method(name = "newMethod")]
    async fn new_method(
        &self,
        param: String,
    ) -> Result<SomeResponse, jsonrpsee::types::ErrorObjectOwned>;
}
```

### 2. Add request/response types (if needed)

Add them to `src/rpc/api.rs` with appropriate `Serialize`/`Deserialize` derives.

### 3. Implement the handler

In `src/rpc/server.rs`, add the method to `VaultRpcServer for VaultRpcImpl<V>`:

```rust
async fn new_method(&self, param: String) -> Result<SomeResponse, ErrorObjectOwned> {
    self.vault
        .new_vault_operation(param)
        .await
        .map_err(vault_to_rpc_err)
}
```

### 4. Add the vault logic (if needed)

Add a method to the `ProofVault` trait (`src/vault/proof_vault.rs`) and implement it in `StandardVault` and the `Arc<V>` blanket impl.

### 5. Update the OpenRPC spec

In `openrpc.json`, add the new method to the `methods` array. Include:
- `name`, `summary`, `description`
- `params` with schemas
- `result` with schema
- `errors` with `$ref` links
- `examples`

### 6. Update the discovery tests

The test in `src/rpc/discovery.rs` asserts that all expected methods are documented:

```rust
for expected in &[
    "vault_storeProof",
    "vault_retrieveProof",
    "vault_deleteProof",
    "rpc.discover",
    "vault_newMethod",  // ← add here
] {
```

### 7. Verify

```bash
make ci
```

---

## Updating the OpenRPC Spec

After any API change (new method, changed params, new error code), update `openrpc.json` and then verify:

```bash
make check-openrpc
```

The script validates:
1. Valid JSON syntax
2. Required OpenRPC fields (`openrpc`, `info.title`, `info.version`, `methods`)
3. All expected method names present
4. All custom error codes present

The spec is embedded at compile time via `include_str!` in `src/rpc/discovery.rs`. A rebuild is required to serve the updated spec at the `rpc.discover` endpoint.

### CI spec drift check

The CI pipeline runs `make check-openrpc` which catches:
- Missing method documentation
- Missing error codes
- Broken JSON syntax

For a stronger check (verify spec matches actual code), add a CI step that:
1. Runs `cargo test` (includes `all_vault_methods_are_documented` and `all_custom_error_codes_are_documented` tests)
2. Runs `make check-openrpc`

---

## Project Structure

```
tari_vault/
├── Cargo.toml              Workspace manifest + dependencies
├── Makefile                Development task runner
├── openrpc.json            Machine-readable API spec (OpenRPC 1.2.6)
├── src/
│   ├── main.rs             Binary entry point (CLI, startup, shutdown)
│   ├── lib.rs              Library root (re-exports)
│   ├── auth.rs             BearerAuthLayer (Tower HTTP middleware)
│   ├── config.rs           Configuration loading (layered)
│   ├── error.rs            VaultError, StorageError
│   ├── domain/
│   │   ├── mod.rs
│   │   ├── claim_id.rs     ClaimId: encode/decode, ZeroizeOnDrop
│   │   ├── proof.rs        PlaintextProof: memory-safe container
│   │   └── record.rs       StoredRecord, EncryptedRecord (serde)
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── backend.rs      StorageBackend trait (RPITIT)
│   │   └── local_file.rs   LocalFileStore: atomic writes, dual locking
│   ├── vault/
│   │   ├── mod.rs
│   │   ├── proof_vault.rs  ProofVault trait + StandardVault + Arc<V> blanket
│   │   └── cleanup.rs      CleanupTask: background TTL sweep
│   └── rpc/
│       ├── mod.rs
│       ├── api.rs          VaultRpc trait (jsonrpsee proc-macro)
│       ├── discovery.rs    rpc.discover handler + compiled-in openrpc.json
│       └── server.rs       Server startup + middleware wiring
├── tests/
│   └── rpc_integration.rs  End-to-end HTTP JSON-RPC tests
├── scripts/
│   └── check_openrpc.sh    OpenRPC spec validation script
└── docs/
    ├── architecture/       System design, security model, data-flow diagrams
    ├── api/                JSON-RPC human reference
    ├── guides/             Getting started, library integration, configuration
    └── development/        This file
```

---

## Dependency Philosophy

Dependencies are chosen for maturity, security record, and minimal footprint:

| Crate | Purpose | Notes |
|-------|---------|-------|
| `aes-gcm` | AES-256-GCM encryption | RustCrypto project |
| `zeroize` | Memory wiping | Industry standard for key material |
| `subtle` | Constant-time comparison | Timing-safe auth token check |
| `jsonrpsee` | JSON-RPC server | async, tower-compatible |
| `tower` | HTTP middleware | Bearer auth layer |
| `tokio` | Async runtime | Multi-threaded, full features |
| `tokio-util` | CancellationToken | Scoped task cancellation |
| `fd-lock` | Cross-platform file locking | Inter-process mutex |
| `config` | Layered configuration | TOML/YAML/env support |
| `serde` / `serde_json` | Serialisation | Vault file format |
| `thiserror` | Error types | Ergonomic derive |
| `anyhow` | Error propagation in binary | Context chains |
| `clap` | CLI parsing | derive-mode |
| `tracing` / `tracing-subscriber` | Structured logging | Async-aware, `RUST_LOG` env filter |
