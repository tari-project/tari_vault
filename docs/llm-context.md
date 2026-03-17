# Tari Vault — LLM/AI Agent Context

> **Purpose of this file:** Dense, structured reference for AI agents and LLMs.
> Contains all information needed to call the API, handle errors, and integrate the library.
> No prose padding. All facts are precise and current.

---

## What Tari Vault Is

A JSON-RPC 2.0 HTTP service (and embeddable Rust library) that:
1. Accepts L1 Merkle Proofs from a sender, encrypts them, returns a `Claim_ID` token.
2. Releases the plaintext only to the holder of the `Claim_ID` — exactly once.
3. Never writes the encryption key to disk.

**Use case:** L1→L2 bridge handoff. The `Claim_ID` can travel through untrusted channels (AI agents, message queues, orchestration pipelines). Only the receiver needs to present it.

---

## Transport

```
Protocol:      HTTP/1.1
Method:        POST
Path:          /
Content-Type:  application/json
Default URL:   http://127.0.0.1:9000
```

All requests use JSON-RPC 2.0 envelope:

```json
{"jsonrpc":"2.0","method":"<method>","params":[<args>],"id":<int>}
```

---

## Authentication

Optional. When enabled:

```
Header:   Authorization: Bearer <token>
Missing:  HTTP 401 + WWW-Authenticate: Bearer realm="tari_vault"
```

Auth is enforced at the HTTP layer before JSON-RPC parsing. All methods (including `rpc.discover`) require the token when enabled.

---

## Claim_ID Token

```
Type:     string
Length:   exactly 64 characters
Charset:  base64url (A-Z a-z 0-9 _ -), no padding
Encodes:  bytes[0..16] = UUIDv4 storage key (non-sensitive)
          bytes[16..48] = AES-256-GCM decryption key (SECRET)
Security: treat as a password; single-use
```

---

## Methods

### vault_storeProof

Store an encrypted proof. Returns a `Claim_ID`.

```
Request params:  positional, one argument (object)
```

```json
{
  "jsonrpc": "2.0",
  "method": "vault_storeProof",
  "params": [{
    "proof_json": <any JSON value>,
    "expires_in_secs": <integer ≥ 0 | null>
  }],
  "id": 1
}
```

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `proof_json` | any JSON | yes | Stored verbatim; not inspected |
| `expires_in_secs` | integer ≥ 0 | no | Omit or null = never expires |

```json
{"jsonrpc":"2.0","result":"<64-char-claim-id>","id":1}
```

Errors: `-32005` (storage failure)

---

### vault_retrieveProof

Retrieve and consume a proof. **Deletes the record. Single-use.**

```json
{
  "jsonrpc": "2.0",
  "method": "vault_retrieveProof",
  "params": ["<claim_id>"],
  "id": 2
}
```

```json
{"jsonrpc":"2.0","result":{"proof_json":<original JSON value>},"id":2}
```

Errors:

| Code | When |
|------|------|
| `-32001` | Proof consumed, deleted, or never existed |
| `-32002` | TTL elapsed |
| `-32003` | Malformed `Claim_ID` (bad base64, wrong length) |
| `-32004` | AES-GCM auth tag mismatch (wrong key or corruption) |
| `-32005` | Storage error |

---

### vault_deleteProof

Delete a stored proof without retrieving it. Abort / cancel flow.

```json
{
  "jsonrpc": "2.0",
  "method": "vault_deleteProof",
  "params": ["<claim_id>"],
  "id": 3
}
```

```json
{"jsonrpc":"2.0","result":null,"id":3}
```

Errors: `-32001` (not found), `-32003` (malformed ID), `-32005` (storage error)

---

### rpc.discover

Return the full OpenRPC document for this service.

```json
{"jsonrpc":"2.0","method":"rpc.discover","params":[],"id":1}
```

Result: OpenRPC document object.

---

## All Error Codes

### Custom

| Code | Name | Meaning |
|------|------|---------|
| `-32001` | ProofNotFound | Token does not exist in storage |
| `-32002` | ProofExpired | TTL elapsed; record purged |
| `-32003` | InvalidClaimId | Bad base64url or length ≠ 64 chars |
| `-32004` | DecryptionFailed | AES-GCM failed (wrong key or corrupted data) |
| `-32005` | InternalError | Storage I/O or serialisation error |

### Standard JSON-RPC

| Code | Name |
|------|------|
| `-32700` | ParseError |
| `-32600` | InvalidRequest |
| `-32601` | MethodNotFound |
| `-32602` | InvalidParams |
| `-32603` | InternalError |

---

## Complete curl Examples

```bash
BASE_URL="http://127.0.0.1:9000"
AUTH=""                            # or: AUTH='-H "Authorization: Bearer <token>"'

# Store
CLAIM_ID=$(curl -s -X POST $BASE_URL $AUTH \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"vault_storeProof","params":[{"proof_json":{"root":"abc","leaf":"def"},"expires_in_secs":3600}],"id":1}' \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['result'])")

echo "Claim_ID: $CLAIM_ID"

# Retrieve
curl -s -X POST $BASE_URL $AUTH \
  -H "Content-Type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"method\":\"vault_retrieveProof\",\"params\":[\"$CLAIM_ID\"],\"id\":2}"

# Delete (abort)
curl -s -X POST $BASE_URL $AUTH \
  -H "Content-Type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"method\":\"vault_deleteProof\",\"params\":[\"$CLAIM_ID\"],\"id\":3}"
```

---

## Rust Library — Quick Reference

```toml
# Cargo.toml
[dependencies]
tari_vault = { path = "../tari_vault" }
```

### Key types

```rust
// Public types
tari_vault::vault::ProofVault          // trait — store/retrieve/delete/cleanup
tari_vault::vault::StandardVault<B>   // concrete impl; B: StorageBackend
tari_vault::vault::CleanupTask        // handle to background sweep task
tari_vault::vault::spawn_cleanup_task // spawn fn
tari_vault::storage::LocalFileStore   // file-backed StorageBackend
tari_vault::storage::SqliteStore      // SQLite-backed StorageBackend
tari_vault::storage::AnyBackend       // enum dispatching to File or Sqlite
tari_vault::storage::StorageBackend   // trait for custom backends
tari_vault::domain::PlaintextProof    // memory-safe proof wrapper
tari_vault::error::VaultError         // all vault errors
tari_vault::auth::BearerAuthLayer     // Tower HTTP middleware
tari_vault::rpc::start_server         // start JSON-RPC server
tari_vault::config::VaultConfig       // full config struct
tari_vault::config::load_config       // load layered config
```

### Minimal usage

```rust
use tari_vault::{
    domain::PlaintextProof,
    storage::LocalFileStore,
    vault::{ProofVault, StandardVault},
};

let vault = StandardVault::new(LocalFileStore::new("vault.json".into())?);

// Store
let proof = PlaintextProof::from_json(&serde_json::json!({"root":"abc"}))?;
let claim_id: String = vault.store_proof(proof, Some(3600)).await?;

// Retrieve (single-use)
let proof = vault.retrieve_proof(claim_id).await?;
let value = proof.into_json()?;

// Delete (abort)
vault.delete_proof(claim_id).await?;

// Cleanup expired
let n: usize = vault.cleanup().await?;
```

### With embedded RPC server

```rust
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tari_vault::{
    rpc::start_server,
    storage::LocalFileStore,
    vault::{ProofVault, StandardVault, spawn_cleanup_task},
};

let vault = Arc::new(StandardVault::new(LocalFileStore::new("vault.json".into())?));
let shutdown = CancellationToken::new();

// Background cleanup
let cleanup = spawn_cleanup_task(Arc::clone(&vault), Duration::from_secs(300), shutdown.clone());

// RPC server (auth_token: None = disabled)
let (addr, handle) = start_server("127.0.0.1:9000", Arc::clone(&vault), None).await?;

// ... on shutdown:
handle.stop()?;
handle.stopped().await;
shutdown.cancel();
cleanup.stopped().await;
```

---

## ProofVault Trait Signatures

```rust
pub trait ProofVault: Send + Sync {
    fn store_proof(&self, proof: PlaintextProof, expires_in_secs: Option<u64>)
        -> impl Future<Output = Result<String, VaultError>> + Send;

    fn retrieve_proof(&self, claim_id_str: String)
        -> impl Future<Output = Result<PlaintextProof, VaultError>> + Send;

    fn cleanup(&self)
        -> impl Future<Output = Result<usize, VaultError>> + Send;

    fn delete_proof(&self, claim_id_str: String)
        -> impl Future<Output = Result<(), VaultError>> + Send;
}
```

Implemented for:
- `StandardVault<B: StorageBackend>`
- `Arc<V: ProofVault>` (blanket impl — enables sharing between RPC server and cleanup task)

---

## StorageBackend Trait

```rust
pub trait StorageBackend: Send + Sync {
    fn insert(&self, record_id: [u8; 16], record: StoredRecord)
        -> impl Future<Output = Result<(), StorageError>> + Send;
    fn fetch(&self, record_id: [u8; 16])
        -> impl Future<Output = Result<StoredRecord, StorageError>> + Send;
    fn delete(&self, record_id: [u8; 16])
        -> impl Future<Output = Result<(), StorageError>> + Send;
    fn delete_expired(&self)
        -> impl Future<Output = Result<usize, StorageError>> + Send;
}
```

---

## VaultError Variants

```rust
pub enum VaultError {
    ProofNotFound,                // rpc_code() = -32001
    ProofExpired,                 // rpc_code() = -32002
    InvalidClaimId,               // rpc_code() = -32003
    DecryptionFailed,             // rpc_code() = -32004  (generic, no oracle)
    Storage(StorageError),        // rpc_code() = -32005
    Serialization(String),        // rpc_code() = -32005
}
```

---

## Configuration

### Priority (low → high)
1. Built-in defaults
2. `vault_config.toml` or `vault_config.yaml` in working directory
3. Environment variables (`VAULT__<SECTION>__<KEY>`)
4. CLI flags

### Environment variables

| Variable | Default | Type |
|----------|---------|------|
| `VAULT__SERVER__BIND_ADDRESS` | `127.0.0.1:9000` | string |
| `VAULT__SERVER__AUTH_TOKEN` | *(null)* | string |
| `VAULT__STORAGE__BACKEND` | `file` | string (`file` or `sqlite`) |
| `VAULT__STORAGE__VAULT_FILE` | platform data dir | path |
| `VAULT__STORAGE__SQLITE_PATH` | same dir as vault_file, `vault.db` | path |
| `VAULT__STORAGE__CLEANUP_INTERVAL_SECS` | `300` | integer |
| `VAULT__LOGGING__LEVEL` | `info` | string |
| `VAULT__LOGGING__CONFIG_FILE` | *(null)* | path |

### CLI flags

```
--config <FILE>           config file path
--vault-file <FILE>       vault file path (file backend)
--sqlite-path <FILE>      SQLite database path (sqlite backend)
--bind <ADDR>             bind address
--cleanup-interval <SECS> cleanup interval (0 = disabled)
--auth-token <TOKEN>      bearer token
--log-config <FILE>       log4rs yaml config
--log-level <LEVEL>       error|warn|info|debug|trace
```

---

## Security Guarantees

| Property | Guarantee |
|----------|-----------|
| Key never on disk | AES-256-GCM key exists only in RAM and in the `Claim_ID` string |
| Single-use | Record deleted on first successful `retrieve_proof` |
| Memory wipe | `ZeroizeOnDrop` on `PlaintextProof`, `ClaimId`, all intermediate key buffers |
| No log leakage | Only `record_id` (non-sensitive UUID) appears in logs |
| Timing-safe auth | Bearer token compared with `subtle::ConstantTimeEq` |
| Crash-safe writes | Atomic rename; old vault file intact until write completes |
| Auth before parsing | HTTP 401 returned before any JSON-RPC processing |

---

## Disk Format

### File backend (`backend = "file"`)

JSON object keyed by hyphenated UUIDv4 strings.

```json
{
  "3f2504e0-4f89-11d3-9a0c-0305e82c3301": {
    "nonce": "<base64-encoded 12 bytes>",
    "ciphertext": "<base64-encoded AES-GCM ciphertext + tag>",
    "stored_at": "2024-01-15T10:30:00Z",
    "expires_at": "2024-01-15T11:30:00Z"
  }
}
```

`expires_at` is absent (JSON `null`) when `expires_in_secs` was not provided.

### SQLite backend (`backend = "sqlite"`)

Single table `proofs` with a partial index on `expires_at`:

```sql
CREATE TABLE proofs (
    record_id   BLOB NOT NULL PRIMARY KEY,  -- 16-byte UUID
    nonce       BLOB NOT NULL,              -- 12 bytes
    ciphertext  BLOB NOT NULL,
    stored_at   TEXT NOT NULL,              -- RFC 3339 UTC
    expires_at  TEXT                        -- RFC 3339 UTC, NULL = no expiry
);
CREATE INDEX idx_expires_at ON proofs (expires_at) WHERE expires_at IS NOT NULL;
```

WAL mode enabled; `PRAGMA secure_delete = ON` so deleted rows are overwritten with zeros.

The encryption key is absent from both storage formats.
Permissions: `0600` on Unix (owner read/write only).

---

## OpenRPC Spec

- **File:** `openrpc.json` at project root
- **Runtime endpoint:** `rpc.discover` JSON-RPC method
- **Playground:** paste `openrpc.json` at https://playground.open-rpc.org/
- **Validate:** `make check-openrpc` or `./scripts/check_openrpc.sh`

---

## Makefile Targets

```
make build           cargo build (debug)
make build-release   cargo build --release
make test            all tests
make test-unit       unit tests only
make test-integration integration tests only
make lint            cargo clippy -D warnings
make fmt             cargo fmt
make fmt-check       cargo fmt --check (CI)
make check           fmt-check + lint + test
make run             cargo run (default settings)
make run-debug       cargo run -- --log-level debug
make check-openrpc   validate openrpc.json
make ci              fmt-check + lint + test + check-openrpc
```
