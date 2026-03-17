## Project Overview

Tari Vault is a Rust-based secure intermediary for handing off encrypted L1 Merkle Proofs to L2 wallets via JSON-RPC 2.0. It uses a **Key-in-the-ID** pattern: the `Claim_ID` returned to callers embeds both a storage lookup key and an AES-256-GCM encryption key. The server only stores ciphertext — encryption keys never touch disk. Claims are single-use (deleted after first retrieval).

## Common Commands

```bash
make build              # Debug build
make build-release      # Release build
make test               # All tests (unit + integration + doctests)
make test-unit          # Unit tests only (cargo test --lib)
make test-integration   # Integration tests only (cargo test --test rpc_integration)
make lint               # Clippy with -D warnings
make fmt                # Format code
make fmt-check          # Check formatting (CI)
make check              # Pre-commit: fmt-check + lint + test
make ci                 # Full CI: fmt-check + lint + test + check-openrpc
make run                # Start vault server
make run-debug          # Start with debug logging
```

Run a single test:
```bash
cargo test test_name_here
```

## Architecture

```
CLI / Binary (main.rs)          — clap, config, log4rs
  ↓
HTTP Transport                  — Tower BearerAuthLayer (constant-time comparison)
  ↓
JSON-RPC Layer (rpc/)           — jsonrpsee 0.24
  ├─ vault_storeProof
  ├─ vault_retrieveProof
  ├─ vault_deleteProof
  └─ rpc.discover               — serves compile-time embedded openrpc.json
  ↓
Vault Core (vault/)             — ProofVault trait, StandardVault<B>, CleanupTask
  ↓
Storage Backend (storage/)      — StorageBackend trait, LocalFileStore (JSON file + fs2 locking)
```

### Key Modules

| Module | Path | Role |
|--------|------|------|
| `config` | `src/config.rs` | Layered config: defaults → file → env → CLI |
| `domain` | `src/domain/` | `PlaintextProof` (ZeroizeOnDrop), `ClaimId` (key-in-ID encode/decode), `StoredRecord` |
| `error` | `src/error.rs` | `VaultError` + `StorageError` with JSON-RPC error code mapping |
| `storage` | `src/storage/` | `StorageBackend` async trait, `LocalFileStore` (dual-lock: tokio Mutex + fs2) |
| `vault` | `src/vault/` | `ProofVault` trait, `StandardVault` impl, `CleanupTask` (periodic TTL sweep) |
| `auth` | `src/auth.rs` | Tower HTTP middleware; validates Bearer token before RPC parsing |
| `rpc` | `src/rpc/` | `VaultRpc` jsonrpsee trait (`api.rs`), discovery (`discovery.rs`), server startup (`server.rs`) |

### Key Design Patterns

- **RPITIT async traits** (Rust 2024 edition) — no `async_trait` macro for `ProofVault` and `StorageBackend`
- **`Arc<V>` blanket impl** on `ProofVault` — allows sharing vault between RPC server and cleanup task
- **Atomic file writes** — all mutations use `NamedTempFile::persist()` (temp + rename)
- **Dual-level locking** — `tokio::sync::Mutex` (intra-process) + `fs2` file lock (inter-process)
- **Compile-time OpenRPC** — `include_str!("../../openrpc.json")` embedded in binary, served via `rpc.discover`
- **ZeroizeOnDrop** on all sensitive types — `PlaintextProof`, `ClaimId`, intermediate buffers

### Claim_ID Structure

```
base64url_nopad( record_id[16 bytes] || encryption_key[32 bytes] )
```

`record_id` is stored on disk as the lookup key; `encryption_key` is never persisted.

## Configuration

Layered (low → high priority): built-in defaults → config file → env vars (`VAULT__SERVER__BIND_ADDRESS`, etc.) → CLI flags.

## Testing Notes

- Unit tests live alongside source in each module
- Integration tests in `tests/rpc_integration.rs` — spin up a real JSON-RPC server per test
- All tests use `tempfile::TempDir` for isolated storage
- `openrpc.json` validation via `scripts/check_openrpc.sh`
