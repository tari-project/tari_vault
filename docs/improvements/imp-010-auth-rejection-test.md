# IMP-010: Assert HTTP 401 in Auth Rejection Tests

**Status:** `[x]` Done
**Tier:** 5 — Test Coverage
**Priority:** Low

## Problem

The integration tests `auth_missing_token_is_rejected` and `auth_wrong_token_is_rejected` (in `tests/rpc_integration.rs`) only assert `result.is_err()`. This assertion would pass if the server returned an HTTP 500 Internal Server Error or any other transport-level failure. The tests do not verify that the response is specifically an HTTP 401 with a `WWW-Authenticate` header, which is the correct and specified behavior of `BearerAuthService`.

## Goal

Strengthen the auth rejection tests to assert the specific HTTP 401 response code and, optionally, the `WWW-Authenticate: Bearer realm="tari_vault"` header.

## Proposed Fix

Use a raw HTTP client (e.g., `reqwest`) in the test instead of the jsonrpsee typed client, so the HTTP status code is accessible:

```rust
let client = reqwest::Client::new();
let resp = client
    .post(server_url)
    .json(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "vault_storeProof",
        "params": [...],
        "id": 1
    }))
    // Intentionally omit Authorization header
    .send()
    .await
    .unwrap();

assert_eq!(resp.status(), 401);
assert!(resp.headers()
    .get("WWW-Authenticate")
    .and_then(|v| v.to_str().ok())
    .map(|v| v.contains("Bearer"))
    .unwrap_or(false));
```

## Affected Files

- `tests/rpc_integration.rs`
- `Cargo.toml` — `reqwest` as a `dev-dependency` (if not already present)

## Notes

- `reqwest` may already be available transitively; check `Cargo.lock` before adding it explicitly.
- Alternatively, the jsonrpsee client may expose the underlying transport error type — check if `ClientError::Transport` carries the HTTP status code in version 0.26.
