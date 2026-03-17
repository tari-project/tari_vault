use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request body for `vault_storeProof`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoreProofParams {
    /// The proof material as any JSON value (object, string, array, …).
    pub proof_json: Value,

    /// Optional time-to-live in seconds.
    /// If omitted the proof never expires.
    pub expires_in_secs: Option<u64>,
}

/// Response body for `vault_retrieveProof`.
#[derive(Debug, Clone, Serialize)]
pub struct ProofObject {
    pub proof_json: Value,
}

/// JSON-RPC 2.0 interface for the vault.
///
/// The `#[rpc(server)]` macro generates a `VaultRpcServer` trait that must be
/// implemented by the concrete handler struct.
#[rpc(server, namespace = "vault")]
pub trait VaultRpc {
    /// Store a proof and return its opaque `Claim_ID` token.
    ///
    /// The token must be passed to `vault_retrieveProof` exactly once.
    #[method(name = "storeProof")]
    async fn store_proof(
        &self,
        params: StoreProofParams,
    ) -> Result<String, jsonrpsee::types::ErrorObjectOwned>;

    /// Retrieve and consume a proof using its `Claim_ID`.
    ///
    /// This is a single-use operation — the record is deleted on success.
    #[method(name = "retrieveProof")]
    async fn retrieve_proof(
        &self,
        claim_id: String,
    ) -> Result<ProofObject, jsonrpsee::types::ErrorObjectOwned>;

    /// Abort / cancel a stored proof without decrypting it.
    ///
    /// The holder of the `Claim_ID` can call this to discard an unused proof
    /// when the associated bridge operation is abandoned.  Returns an error
    /// if the proof has already been retrieved, deleted, or was never stored.
    #[method(name = "deleteProof")]
    async fn delete_proof(
        &self,
        claim_id: String,
    ) -> Result<(), jsonrpsee::types::ErrorObjectOwned>;
}
