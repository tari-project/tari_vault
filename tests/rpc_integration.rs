/// End-to-end integration tests that spin up a real JSON-RPC HTTP server,
/// send curl-equivalent requests via jsonrpsee's HTTP client, and verify the
/// full store → retrieve lifecycle.
use http::{HeaderMap, HeaderValue, header::AUTHORIZATION};
use jsonrpsee::{
    core::client::ClientT, http_client::HttpClientBuilder, rpc_params, server::ServerHandle,
};
use serde_json::{Value, json};
use tari_vault::{
    rpc::{api::StoreProofParams, start_server},
    storage::LocalFileStore,
    vault::StandardVault,
};
use tempfile::TempDir;

async fn start_test_server(auth_token: Option<String>) -> (String, ServerHandle, TempDir) {
    let dir = TempDir::new().unwrap();
    let storage = LocalFileStore::new(dir.path().join("vault.json")).unwrap();
    let vault = StandardVault::new(storage);
    let (addr, handle) = start_server("127.0.0.1:0", vault, auth_token)
        .await
        .unwrap();
    (format!("http://{addr}"), handle, dir)
}

fn bearer_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    headers
}

/// Extract the JSON-RPC error code from a jsonrpsee client error.
fn rpc_error_code(err: &jsonrpsee::core::ClientError) -> Option<i32> {
    match err {
        jsonrpsee::core::ClientError::Call(obj) => Some(obj.code()),
        _ => None,
    }
}

#[tokio::test]
async fn store_and_retrieve_via_rpc() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let proof_value = json!({"root": "deadbeef", "path": [1, 2, 3]});

    // ── vault_storeProof ────────────────────────────────────────────────────
    let params = StoreProofParams {
        proof_json: proof_value.clone(),
        expires_in_secs: None,
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .expect("storeProof failed");

    assert_eq!(claim_id.len(), 64, "ClaimId must be 64 base64url chars");

    // ── vault_retrieveProof ─────────────────────────────────────────────────
    let result: Value = client
        .request("vault_retrieveProof", rpc_params![claim_id.clone()])
        .await
        .expect("retrieveProof failed");

    assert_eq!(result["proof_json"], proof_value);
}

#[tokio::test]
async fn second_retrieval_returns_not_found() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let params = StoreProofParams {
        proof_json: json!("single-use-proof"),
        expires_in_secs: None,
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .unwrap();

    // First retrieval succeeds.
    let _: Value = client
        .request("vault_retrieveProof", rpc_params![claim_id.clone()])
        .await
        .unwrap();

    // Second retrieval must return a ProofNotFound RPC error.
    let err = client
        .request::<Value, _>("vault_retrieveProof", rpc_params![claim_id])
        .await
        .expect_err("Expected ProofNotFound error on second retrieval");

    let code = rpc_error_code(&err).expect("should be an RPC Call error");
    assert_eq!(code, -32001, "Expected ProofNotFound (-32001), got {code}");
}

#[tokio::test]
async fn retrieve_with_invalid_claim_id_returns_error() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let err = client
        .request::<Value, _>("vault_retrieveProof", rpc_params!["not-a-valid-claim-id"])
        .await
        .expect_err("Expected error for invalid claim ID");

    let code = rpc_error_code(&err).expect("should be an RPC Call error");
    assert_eq!(code, -32003, "Expected InvalidClaimId (-32003), got {code}");
}

#[tokio::test]
async fn store_with_ttl_is_retrievable_immediately() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    // A 1-hour TTL should be stored and immediately retrievable.
    let params = StoreProofParams {
        proof_json: json!({"data": "with-ttl"}),
        expires_in_secs: Some(3600),
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .expect("storeProof with TTL failed");

    let result: Value = client
        .request("vault_retrieveProof", rpc_params![claim_id])
        .await
        .expect("retrieveProof with TTL failed");

    assert_eq!(result["proof_json"]["data"], "with-ttl");
}

#[tokio::test]
async fn proof_json_accepts_arbitrary_json_types() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    for proof in [
        json!("plain string proof"),
        json!(42),
        json!({"nested": {"key": "value"}, "array": [1, 2, 3]}),
        json!(["a", "b", "c"]),
    ] {
        let params = StoreProofParams {
            proof_json: proof.clone(),
            expires_in_secs: None,
        };
        let claim_id: String = client
            .request("vault_storeProof", rpc_params![params])
            .await
            .unwrap();
        let result: Value = client
            .request("vault_retrieveProof", rpc_params![claim_id])
            .await
            .unwrap();
        assert_eq!(result["proof_json"], proof);
    }
}

#[tokio::test]
async fn delete_proof_removes_it_and_retrieval_fails() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let params = StoreProofParams {
        proof_json: json!({"root": "to-be-aborted"}),
        expires_in_secs: None,
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .unwrap();

    // Explicit abort — caller decides to cancel the bridge operation.
    let result: Value = client
        .request("vault_deleteProof", rpc_params![claim_id.clone()])
        .await
        .expect("deleteProof should succeed");
    assert!(result.is_null(), "deleteProof should return null (unit)");

    // Subsequent retrieval must return ProofNotFound.
    let err = client
        .request::<Value, _>("vault_retrieveProof", rpc_params![claim_id])
        .await
        .expect_err("Expected ProofNotFound after explicit delete");
    let code = rpc_error_code(&err).expect("should be an RPC Call error");
    assert_eq!(code, -32001, "Expected ProofNotFound (-32001), got {code}");
}

#[tokio::test]
async fn delete_proof_after_retrieval_returns_not_found() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let params = StoreProofParams {
        proof_json: json!("consume-then-delete"),
        expires_in_secs: None,
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .unwrap();

    // Retrieve the proof (single-use consumption).
    let _: Value = client
        .request("vault_retrieveProof", rpc_params![claim_id.clone()])
        .await
        .unwrap();

    // Delete on an already-consumed proof must return ProofNotFound (-32001).
    let err = client
        .request::<Value, _>("vault_deleteProof", rpc_params![claim_id])
        .await
        .expect_err("Expected ProofNotFound on double-delete");
    let code = rpc_error_code(&err).expect("should be an RPC Call error");
    assert_eq!(code, -32001, "Expected ProofNotFound (-32001), got {code}");
}

#[tokio::test]
async fn retrieve_expired_proof_returns_proof_expired() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    // Store with TTL=0 so it expires immediately.
    let params = StoreProofParams {
        proof_json: json!("expires now"),
        expires_in_secs: Some(0),
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .unwrap();

    // Wait for the clock to advance past the zero-second mark.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let err = client
        .request::<Value, _>("vault_retrieveProof", rpc_params![claim_id])
        .await
        .expect_err("Expected error for expired proof");

    let code = rpc_error_code(&err).expect("should be an RPC Call error");
    assert!(
        code == -32002 || code == -32001,
        "Expected ProofExpired (-32002) or ProofNotFound (-32001), got {code}"
    );
}

#[tokio::test]
async fn delete_with_invalid_claim_id_returns_error() {
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let err = client
        .request::<Value, _>("vault_deleteProof", rpc_params!["not-a-valid-claim-id"])
        .await
        .expect_err("Expected error for invalid claim ID on delete");

    let code = rpc_error_code(&err).expect("should be an RPC Call error");
    assert_eq!(code, -32003, "Expected InvalidClaimId (-32003), got {code}");
}

// ── Authentication tests ──────────────────────────────────────────────────────

const TEST_TOKEN: &str = "test-bearer-token-abc123";

#[tokio::test]
async fn auth_valid_token_allows_requests() {
    let (url, _handle, _dir) = start_test_server(Some(TEST_TOKEN.to_string())).await;
    let client = HttpClientBuilder::default()
        .set_headers(bearer_headers(TEST_TOKEN))
        .build(&url)
        .unwrap();

    let params = StoreProofParams {
        proof_json: json!("authed proof"),
        expires_in_secs: None,
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .expect("authenticated request should succeed");

    assert_eq!(claim_id.len(), 64);
}

#[tokio::test]
async fn auth_missing_token_is_rejected() {
    let (url, _handle, _dir) = start_test_server(Some(TEST_TOKEN.to_string())).await;
    // No Authorization header.
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let params = StoreProofParams {
        proof_json: json!("unauthenticated"),
        expires_in_secs: None,
    };
    let result = client
        .request::<String, _>("vault_storeProof", rpc_params![params])
        .await;

    assert!(result.is_err(), "request without token should be rejected");
}

#[tokio::test]
async fn auth_wrong_token_is_rejected() {
    let (url, _handle, _dir) = start_test_server(Some(TEST_TOKEN.to_string())).await;
    let client = HttpClientBuilder::default()
        .set_headers(bearer_headers("wrong-token"))
        .build(&url)
        .unwrap();

    let params = StoreProofParams {
        proof_json: json!("wrong token"),
        expires_in_secs: None,
    };
    let result = client
        .request::<String, _>("vault_storeProof", rpc_params![params])
        .await;

    assert!(
        result.is_err(),
        "request with wrong token should be rejected"
    );
}

#[tokio::test]
async fn auth_disabled_needs_no_token() {
    // Server started with auth_token = None — no header required.
    let (url, _handle, _dir) = start_test_server(None).await;
    let client = HttpClientBuilder::default().build(&url).unwrap();

    let params = StoreProofParams {
        proof_json: json!("no auth needed"),
        expires_in_secs: None,
    };
    let claim_id: String = client
        .request("vault_storeProof", rpc_params![params])
        .await
        .expect("unauthenticated request should succeed when auth is disabled");

    assert_eq!(claim_id.len(), 64);
}
