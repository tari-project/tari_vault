# Axum Integration Guide

This guide covers embedding the `tari_vault` JSON-RPC endpoint into an **existing axum HTTP service** rather than starting a dedicated TCP listener.

Use this approach when your `walletd` (or other host process) already runs an axum router and you want to mount the vault at a sub-path (e.g. `/vault/rpc`) instead of on a separate port.

---

## How it works

`jsonrpsee` 0.24 can expose any `RpcModule` as a Tower service via
`Server::builder().to_service_builder()`. The resulting service has no
listener of its own — it is handed to axum's `Router::route_service`, and
your existing axum server drives all I/O.

The vault's `BearerAuthLayer` remains unchanged: it is passed as the HTTP
middleware when building the service, so auth is enforced before any
JSON-RPC parsing, exactly as in the standalone mode.

---

## Additional dependencies

In your host crate's `Cargo.toml`, add `jsonrpsee` with the version that
matches what `tari_vault` uses:

```toml
[dependencies]
tari_vault  = { git = "https://github.com/tari-project/tari_vault/", branch = "main" }
jsonrpsee   = { version = "0.24", features = ["server"] }
tokio-util  = { version = "0.7", features = ["rt"] }
```

---

## Complete example

```rust
use std::{sync::Arc, time::Duration};

use axum::Router;
use jsonrpsee::server::{stop_channel, Server};
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tari_vault::{
    auth::BearerAuthLayer,
    rpc::{
        api::VaultRpcServer,           // brings .into_rpc() into scope
        discovery::discovery_module,
        server::VaultRpcImpl,
    },
    storage::LocalFileStore,
    vault::{ProofVault, StandardVault, spawn_cleanup_task},
};

pub struct VaultSubsystem {
    /// Signal the jsonrpsee service to drain in-flight requests.
    server_handle: jsonrpsee::server::ServerHandle,
    cleanup_task: tari_vault::vault::CleanupTask,
}

impl VaultSubsystem {
    /// Build the vault Tower service and merge it into `router`.
    ///
    /// Returns the augmented router and a handle used during shutdown.
    pub async fn mount(
        router: Router,
        vault_file: std::path::PathBuf,
        auth_token: Option<String>,
        cleanup_interval: Duration,
        shutdown: CancellationToken,
    ) -> anyhow::Result<(Router, Self)> {
        let storage = LocalFileStore::new(vault_file)?;
        let vault = Arc::new(StandardVault::new(storage));

        // Startup sweep.
        let purged = vault.cleanup().await?;
        if purged > 0 {
            log::info!("Startup cleanup removed {purged} expired proof(s)");
        }

        // Background TTL sweep.
        let cleanup_task = spawn_cleanup_task(
            Arc::clone(&vault),
            cleanup_interval,
            shutdown.clone(),
        );

        // Build the RPC module (vault methods + rpc.discover).
        let mut rpc_module = VaultRpcImpl::new(Arc::clone(&vault)).into_rpc();
        rpc_module.merge(discovery_module())?;

        // Auth middleware (transparent when auth_token is None).
        let auth = BearerAuthLayer::from_config(auth_token);
        let middleware = ServiceBuilder::new().layer(auth);

        // Create a (stop_handle, server_handle) pair for graceful shutdown.
        let (stop_handle, server_handle) = stop_channel();

        // Build the Tower service — no TCP listener; axum drives the socket.
        let rpc_service = Server::builder()
            .set_http_middleware(middleware)
            .to_service_builder()
            .build(rpc_module, stop_handle);

        // Mount under a chosen path.
        let router = router.route_service("/vault/rpc", rpc_service);

        log::info!("Vault RPC mounted at /vault/rpc");
        Ok((router, Self { server_handle, cleanup_task }))
    }

    pub async fn stop(self) {
        // Tell jsonrpsee to stop accepting new requests and drain existing ones.
        self.server_handle.stop().ok();
        self.server_handle.stopped().await;
        // The cleanup task observes the same CancellationToken passed to mount().
        self.cleanup_task.stopped().await;
        log::info!("Vault subsystem stopped");
    }
}
```

### Wiring it into your walletd main

```rust
use std::{path::PathBuf, time::Duration};
use axum::Router;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let shutdown = CancellationToken::new();

    // Start with your existing walletd routes.
    let base_router = Router::new()
        /* .route("/health", ...) */;

    // Attach the vault at /vault/rpc.
    let (app, vault) = VaultSubsystem::mount(
        base_router,
        PathBuf::from("/var/lib/walletd/vault.json"),
        std::env::var("VAULT_AUTH_TOKEN").ok(),
        Duration::from_secs(300),
        shutdown.clone(),
    )
    .await?;

    // Single listener, single axum server.
    let listener = TcpListener::bind("127.0.0.1:9001").await?;
    axum::serve(listener, app)
        .with_graceful_shutdown({
            let shutdown = shutdown.clone();
            async move { shutdown.cancelled().await }
        })
        .await?;

    // Drain the vault after the listener closes.
    vault.stop().await;
    Ok(())
}
```

---

## Auth behaviour

`BearerAuthLayer` operates at the Tower HTTP layer, so the vault service
enforces `Authorization: Bearer <token>` on **every** request before any
JSON-RPC parsing takes place — identical to the standalone-server mode.

If your axum router already has global auth middleware, you can pass
`None` for `auth_token` and rely on the outer middleware instead.  The
layer becomes transparent when disabled, so it is always safe to install.

---

## Shutdown ordering

| Step | What happens |
|------|-------------|
| 1 | Your shutdown signal fires (e.g. SIGTERM, `CancellationToken::cancel`) |
| 2 | `axum::serve(...).with_graceful_shutdown(...)` stops accepting new connections |
| 3 | `vault.stop()` → `server_handle.stop()` drains in-flight vault RPC requests |
| 4 | `cleanup_task.stopped()` waits for the background TTL sweep to exit |

Call `vault.stop()` **after** `axum::serve` returns so that the server has
already stopped routing new requests before jsonrpsee is told to drain.

---

## Path and routing notes

- The vault is mounted at a single path (`/vault/rpc` in the example); adjust
  to match your API layout.
- `POST` is the only method used by JSON-RPC 2.0; `GET` requests to that path
  will return 405.
- The `rpc.discover` method (OpenRPC service discovery) is included
  automatically via `discovery_module()` and is accessible through the same
  path.
