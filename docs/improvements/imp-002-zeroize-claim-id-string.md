# IMP-002: Zeroize Incoming `claim_id` String

**Status:** `[x]` Completed
**Tier:** 1 — Security
**Priority:** High

## Problem

In `StandardVault::retrieve_proof` (`src/vault/proof_vault.rs`) and `delete_proof`, the `claim_id_str: String` parameter carries the full base64url-encoded `Claim_ID`, which embeds the 32-byte AES-256-GCM key. After `ClaimId::decode()` extracts and zeroizes the key material into a `Zeroizing<Vec<u8>>`, the original `String` is dropped with a plain `drop()` call.

`String` does not implement `Zeroize`. The underlying heap allocation is freed but not overwritten, leaving the AES key recoverable from memory dumps, swap files, or core dumps until the allocator reuses those pages.

The decoded `ClaimId` struct is correctly `ZeroizeOnDrop`. Only the raw incoming string is affected.

## Goal

Ensure the base64url-encoded key material is overwritten in memory immediately after decoding, with no lingering plaintext window.

## Proposed Fix

Replace `claim_id_str: String` with `claim_id_str: zeroize::Zeroizing<String>` in the `ProofVault` trait and its `StandardVault` implementation.

```rust
// Before
async fn retrieve_proof(&self, claim_id_str: String) -> Result<PlaintextProof, VaultError>;

// After
async fn retrieve_proof(&self, claim_id_str: Zeroizing<String>) -> Result<PlaintextProof, VaultError>;
```

The RPC dispatch layer (`src/rpc/server.rs`) would wrap the deserialized string at the call site:

```rust
let claim_id = Zeroizing::new(params.claim_id);
self.vault.retrieve_proof(claim_id).await
```

## Affected Files

- `src/vault/proof_vault.rs` — trait signature + `StandardVault` impl
- `src/rpc/server.rs` — call sites for `retrieve_proof` and `delete_proof`
- `tests/rpc_integration.rs` — no signature changes expected at the test level

## Notes

- `Zeroizing<String>` implements `Deref<Target = String>`, so internal call sites using `&claim_id_str` require no changes.
- The `ProofVault` trait is also implemented by `Arc<V>` via the blanket impl; the blanket impl forwards unchanged so no duplicate work required.
