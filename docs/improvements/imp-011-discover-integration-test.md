# IMP-011: Integration Test for `rpc.discover`

**Status:** `[ ]` Planned
**Tier:** 5 — Test Coverage
**Priority:** Low

## Problem

`src/rpc/discovery.rs` has a unit test that validates the compile-time embedded `openrpc.json` is parseable JSON. However, there is no integration test that:

1. Calls `rpc.discover` over a live server.
2. Verifies the method is correctly registered and requires auth (returns 401 without a token).
3. Verifies the returned schema is valid OpenRPC (has `openrpc`, `info`, and `methods` fields).

Without this, a regression that breaks the `discovery_module()` registration or the `into_rpc()` merge would go undetected by the test suite.

## Goal

Add a live-server integration test for `rpc.discover`.

## Proposed Test Cases

```rust
#[tokio::test]
async fn discover_returns_valid_openrpc_schema() {
    // Spin up server with auth
    // Call rpc.discover with valid Bearer token
    // Assert result is a JSON object with "openrpc", "info", "methods" keys
    // Assert "methods" array is non-empty
}

#[tokio::test]
async fn discover_requires_auth() {
    // Spin up server with auth
    // Call rpc.discover without token
    // Assert 401 / auth error
}
```

## Affected Files

- `tests/rpc_integration.rs`
