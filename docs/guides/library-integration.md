# Library Integration Guide

This guide is for Rust developers embedding `tari_vault` as a library dependency — most commonly in an L2 wallet daemon (`walletd`).

---

## Two Integration Modes

| Mode | When to use |
|------|------------|
| **Direct (in-process)** | Your application calls `ProofVault` methods directly from Rust code. No HTTP overhead. Best when the vault is a private subsystem of a single process. |
| **Embedded RPC server** | You want to expose the vault over JSON-RPC so other processes (AI agents, bridges, CLI tools) can call it. Start the HTTP server alongside your existing tokio runtime. |

Both modes can be combined: use the vault directly in Rust while also exposing it to external callers via the RPC server.

---

## Adding as a Dependency

In your `Cargo.toml`:

```toml
[dependencies]
tari_vault = { git = "https://github.com/tari-project/tari_vault/", branch = "main" }
```

---

## Mode 1 — Direct In-Process Usage

Use `StandardVault` directly. No HTTP server, no JSON-RPC overhead.

```rust
use std::path::PathBuf;
use tari_vault::{
    domain::PlaintextProof,
    storage::LocalFileStore,
    vault::{ProofVault, StandardVault},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Open (or create) the vault file.
    let storage = LocalFileStore::new(PathBuf::from("/var/lib/walletd/vault.json"))?;
    let vault = StandardVault::new(storage);

    // ── Sender side ──────────────────────────────────────────────────────────
    // proof_json can be any serde_json::Value
    let proof = PlaintextProof::from_json(&serde_json::json!({
        "root": "a1b2c3",
        "path": [{"hash": "ab", "direction": "left"}],
        "leaf": "deadbeef"
    }))?;

    // Store with a 1-hour TTL (pass None for no expiry).
    let claim_id: String = vault.store_proof(proof, Some(3600)).await?;
    // claim_id is a 64-char base64url token — safe to pass through untrusted channels.

    // ── Receiver side ─────────────────────────────────────────────────────────
    // claim_id arrives from the sender (via any channel).
    let proof = vault.retrieve_proof(claim_id).await?;
    let json_value = proof.into_json()?;
    println!("Proof: {json_value}");

    Ok(())
}
```

### Abort / Cancel

```rust
// If the bridge operation is abandoned, delete without retrieving:
vault.delete_proof(claim_id).await?;
// Returns VaultError::ProofNotFound if already consumed or deleted.
```

### Manual cleanup

```rust
// Purge expired proofs on demand (e.g. at startup):
let n = vault.cleanup().await?;
println!("Removed {n} expired proof(s)");
```

---

## Mode 2 — Embedded RPC Server

Expose the vault over JSON-RPC HTTP. Share the vault instance between the RPC server and the background cleanup task.

```rust
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tari_vault::{
    rpc::start_server,
    storage::LocalFileStore,
    vault::{ProofVault, StandardVault, spawn_cleanup_task},
};

pub struct VaultSubsystem {
    server_handle: jsonrpsee::server::ServerHandle,
    cleanup_task: tari_vault::vault::CleanupTask,
    shutdown: CancellationToken,
}

impl VaultSubsystem {
    pub async fn start(
        bind_addr: &str,
        vault_file: std::path::PathBuf,
        auth_token: Option<String>,
        cleanup_interval: Duration,
        shutdown: CancellationToken,
    ) -> anyhow::Result<Self> {
        let storage = LocalFileStore::new(vault_file)?;
        let vault = Arc::new(StandardVault::new(storage));

        // Startup sweep: clear any leftover expired proofs from previous runs.
        let purged = vault.cleanup().await?;
        if purged > 0 {
            tracing::info!("Startup cleanup removed {purged} expired proof(s)");
        }

        // Background sweep.
        let cleanup_task = spawn_cleanup_task(
            Arc::clone(&vault),
            cleanup_interval,
            shutdown.clone(),
        );

        // Start the JSON-RPC server.
        let (addr, server_handle) =
            start_server(bind_addr, Arc::clone(&vault), auth_token).await?;

        tracing::info!("Vault subsystem started on {addr}");

        Ok(Self { server_handle, cleanup_task, shutdown })
    }

    pub async fn stop(self) {
        self.server_handle.stop().ok();
        self.server_handle.stopped().await;
        self.shutdown.cancel();
        self.cleanup_task.stopped().await;
        tracing::info!("Vault subsystem stopped");
    }
}
```

### Integration with walletd shutdown

```rust
// In your walletd main():
let walletd_shutdown = CancellationToken::new();

let vault = VaultSubsystem::start(
    "127.0.0.1:9001",
    PathBuf::from("/var/lib/walletd/vault.json"),
    std::env::var("VAULT_AUTH_TOKEN").ok(),
    Duration::from_secs(300),
    walletd_shutdown.clone(),
).await?;

// ... start the rest of walletd ...

// On shutdown signal:
walletd_shutdown.cancel();
vault.stop().await;
```

---

## Implementing a Custom Storage Backend

`LocalFileStore` is the reference implementation. To use a different backend (database, encrypted filesystem, in-memory for testing):

```rust
use std::future::Future;
use tari_vault::{
    domain::StoredRecord,
    error::StorageError,
    storage::StorageBackend,
    vault::StandardVault,
};

struct MyDatabaseStore { /* ... */ }

impl StorageBackend for MyDatabaseStore {
    fn insert(
        &self,
        record_id: [u8; 16],
        record: StoredRecord,
    ) -> impl Future<Output = Result<(), StorageError>> + Send {
        async move {
            // persist record to your database ...
            Ok(())
        }
    }

    fn fetch(
        &self,
        record_id: [u8; 16],
    ) -> impl Future<Output = Result<StoredRecord, StorageError>> + Send {
        async move {
            // load from database, return StorageError::NotFound if absent
            Err(StorageError::NotFound)
        }
    }

    fn delete(
        &self,
        record_id: [u8; 16],
    ) -> impl Future<Output = Result<(), StorageError>> + Send {
        async move { Ok(()) }
    }

    fn delete_expired(
        &self,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send {
        async move {
            // delete all records where expires_at < Utc::now()
            Ok(0)
        }
    }
}

// Use your backend with StandardVault:
let vault = StandardVault::new(MyDatabaseStore { /* ... */ });
```

`StorageBackend` uses RPITIT (`impl Future<...> + Send` in trait definitions). Implementations use plain `async fn` in the impl block.

---

## Error Handling

All vault methods return `Result<_, VaultError>`. The variants:

```rust
pub enum VaultError {
    ProofNotFound,        // The Claim_ID does not exist (consumed, deleted, or never stored)
    ProofExpired,         // TTL elapsed; proof is gone
    DecryptionFailed,     // Wrong Claim_ID or corrupted ciphertext (generic — no oracle)
    InvalidClaimId,       // Malformed base64url or wrong length
    Storage(StorageError),         // I/O or JSON error from the storage layer
    Serialization(String),         // JSON serialisation error
}
```

Recommended handling:

```rust
use tari_vault::error::VaultError;

match vault.retrieve_proof(claim_id).await {
    Ok(proof) => { /* use proof */ }
    Err(VaultError::ProofNotFound) => { /* already consumed or never existed */ }
    Err(VaultError::ProofExpired)  => { /* TTL elapsed */ }
    Err(VaultError::DecryptionFailed | VaultError::InvalidClaimId) => {
        // Bad Claim_ID — log and reject
    }
    Err(e) => { /* storage/serialisation error — log at error level */ }
}
```

---

## Important: Tokio Runtime Requirement

`tari_vault` requires a multi-threaded Tokio runtime. The `LocalFileStore` uses `tokio::task::spawn_blocking` for all file I/O. Ensure your runtime is started with:

```rust
#[tokio::main]  // uses multi_thread scheduler by default
async fn main() { ... }

// or explicitly:
tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?
    .block_on(async { ... });
```

---

## Logging Integration

`tari_vault` uses the `tracing` crate. Log output appears in whatever `tracing` subscriber your application installs. If you have no subscriber, install one before starting the vault:

```rust
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

Vault log targets:

| Target | Content |
|--------|---------|
| `tari_vault::vault` | Proof stored/retrieved/deleted (record_id only, never key or plaintext) |
| `tari_vault::cleanup` | Periodic sweep results |
| `tari_vault::rpc` | Server address, internal errors |
| `tari_vault::auth` | Rejected unauthenticated requests |

Control verbosity with `RUST_LOG`:

```bash
RUST_LOG=tari_vault=debug,tari_vault::cleanup=info ./your_app
```

All sensitive fields (`Claim_ID`, `encryption_key`, `PlaintextProof`) are explicitly redacted and never appear in log output at any log level.
