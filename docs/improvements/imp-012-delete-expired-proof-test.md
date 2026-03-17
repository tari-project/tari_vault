# IMP-012: Integration Test for `vault_deleteProof` on Expired Proof

**Status:** `[ ]` Planned
**Tier:** 5 — Test Coverage
**Priority:** Low

## Problem

There is no integration test covering the behavior of `vault_deleteProof` when the target proof has already expired. The unit tests in `src/vault/proof_vault.rs` cover delete-after-consume but not the expiry-during-delete path.

The expected behavior: `vault_deleteProof` called with a valid `Claim_ID` for an expired record should return `ProofExpired` (or `ProofNotFound` — this needs a design decision).

## Design Question

When deleting an expired record:
- **Option A:** Return `ProofExpired` (the record is there, but expired). Consistent with `retrieve_proof` behavior.
- **Option B:** Return `ProofNotFound` (from the caller's perspective, an expired proof is functionally absent).

The current `delete_proof` implementation calls `storage.fetch()` first and checks expiry, so it can distinguish the two. Option A is more informative; Option B is simpler. **Recommendation: Option A** (mirror `retrieve_proof` behavior).

## Proposed Test

```rust
#[tokio::test]
async fn delete_expired_proof_returns_expired_error() {
    // Store a proof with expires_in_secs: Some(1)
    // Sleep > 1 second
    // Call vault_deleteProof with the claim ID
    // Assert error code matches ProofExpired (-32003 or equivalent)
}
```

## Affected Files

- `tests/rpc_integration.rs`
- Possibly `src/vault/proof_vault.rs` — if the expiry-during-delete path needs explicit handling
