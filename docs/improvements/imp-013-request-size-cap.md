# IMP-013: Request Size Cap on `proof_json`

**Status:** `[ ]` Planned
**Tier:** 6 — Minor / Nice-to-Have
**Priority:** Low

## Problem

`PlaintextProof::from_json` accepts any `serde_json::Value` without a size check. An authenticated caller can submit a multi-megabyte JSON payload, which will be:

1. Fully serialized to bytes (`serde_json::to_vec`).
2. AES-256-GCM encrypted (proportional memory allocation).
3. Written to the vault file as base64-encoded ciphertext (adds ~33% overhead).

There is no per-request throttle or vault-size cap, so a single authenticated caller could fill disk with large proofs.

## Goal

Enforce a configurable maximum payload size for `proof_json`.

## Proposed Approach

### Option A: HTTP middleware layer (preferred)

Add a request body size limit in the Tower middleware chain, before JSON-RPC parsing:

```rust
// In server.rs, alongside BearerAuthLayer
tower::limit::RequestBodyLimitLayer::new(1 * 1024 * 1024) // 1 MB
```

This rejects oversized requests at the transport layer with HTTP 413 before any deserialization occurs.

### Option B: Vault-level validation

Add a `max_proof_size_bytes: usize` field to `VaultConfig` and check `proof_bytes.len()` in `StandardVault::store_proof` before encryption:

```rust
if proof_bytes.len() > self.config.max_proof_size_bytes {
    return Err(VaultError::InvalidParameter("proof_json exceeds maximum size"));
}
```

**Recommendation: Option A** for defense-in-depth at the HTTP layer, with Option B as an additional semantic check at the vault level.

## Affected Files

- `src/rpc/server.rs` — add `RequestBodyLimitLayer`
- `src/config.rs` — optional `max_proof_size_bytes` config field
- `Cargo.toml` — `tower` already a dependency; `RequestBodyLimitLayer` is in `tower-http`

## Default Value

1 MB is a reasonable default for Merkle proofs. Should be configurable via `VAULT__SERVER__MAX_PROOF_SIZE_BYTES`.
