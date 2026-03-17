use jsonrpsee::{
    server::{Server, ServerHandle},
    types::ErrorObjectOwned,
};
use std::net::SocketAddr;

use crate::{
    auth::BearerAuthLayer,
    domain::PlaintextProof,
    error::VaultError,
    rpc::{
        api::{ProofObject, StoreProofParams, VaultRpcServer},
        discovery::discovery_module,
    },
    vault::ProofVault,
};

/// Concrete JSON-RPC handler that delegates to a `ProofVault` implementation.
pub struct VaultRpcImpl<V> {
    vault: V,
}

impl<V: ProofVault> VaultRpcImpl<V> {
    pub fn new(vault: V) -> Self {
        Self { vault }
    }
}

#[async_trait::async_trait]
impl<V: ProofVault + 'static> VaultRpcServer for VaultRpcImpl<V> {
    async fn store_proof(&self, params: StoreProofParams) -> Result<String, ErrorObjectOwned> {
        let proof = PlaintextProof::from_json(&params.proof_json).map_err(vault_to_rpc_err)?;

        self.vault
            .store_proof(proof, params.expires_in_secs)
            .await
            .map_err(vault_to_rpc_err)
    }

    async fn retrieve_proof(&self, claim_id: String) -> Result<ProofObject, ErrorObjectOwned> {
        let proof = self
            .vault
            .retrieve_proof(claim_id)
            .await
            .map_err(vault_to_rpc_err)?;

        let proof_json = proof.into_json().map_err(vault_to_rpc_err)?;
        Ok(ProofObject { proof_json })
    }

    async fn delete_proof(&self, claim_id: String) -> Result<(), ErrorObjectOwned> {
        self.vault
            .delete_proof(claim_id)
            .await
            .map_err(vault_to_rpc_err)
    }
}

/// Start the JSON-RPC HTTP server and return a handle for graceful shutdown.
///
/// When `auth_token` is `Some(token)`, every request must carry an
/// `Authorization: Bearer <token>` header; requests without it receive an
/// HTTP 401 before any RPC processing occurs.  Pass `None` to disable auth.
pub async fn start_server(
    bind_addr: &str,
    vault: impl ProofVault + 'static,
    auth_token: Option<String>,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    let auth = BearerAuthLayer::from_config(auth_token);
    let middleware = tower::ServiceBuilder::new().layer(auth);

    let server = Server::builder()
        .set_http_middleware(middleware)
        .build(bind_addr)
        .await?;

    let addr = server.local_addr()?;

    let mut rpc_module = VaultRpcImpl::new(vault).into_rpc();
    rpc_module.merge(discovery_module())?;

    let handle = server.start(rpc_module);
    log::info!(target: "tari_vault::rpc", "Vault RPC server listening on {addr}");
    Ok((addr, handle))
}

// ── Error mapping ────────────────────────────────────────────────────────────

fn vault_to_rpc_err(err: VaultError) -> ErrorObjectOwned {
    let code = err.rpc_code();
    // Log internal errors at warn level — never include sensitive detail.
    if code == -32005 {
        log::warn!(target: "tari_vault::rpc", "Internal vault error: {err}");
    }
    ErrorObjectOwned::owned(code, err.to_string(), None::<()>)
}
