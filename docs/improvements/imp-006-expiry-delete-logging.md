# IMP-006: Log Failed Expiry Deletion During Retrieve

**Status:** `[x]` Completed
**Tier:** 3 — Reliability
**Priority:** Low

## Problem

In `StandardVault::retrieve_proof` (`src/vault/proof_vault.rs`), when a fetched record is found to be expired, the code attempts to delete it and explicitly discards the result:

```rust
let _ = self.storage.delete(record_id).await;
return Err(VaultError::ProofExpired);
```

If the deletion fails (e.g., I/O error, file permissions issue), the expired record accumulates on disk indefinitely. The caller correctly receives `ProofExpired`, so there is no security impact, but the vault file grows without bound until the next successful `CleanupTask` sweep.

## Goal

Surface failed opportunistic deletions in the log so operators are aware of storage-layer issues.

## Proposed Fix

Replace the silent discard with a logged warning:

```rust
if let Err(e) = self.storage.delete(record_id).await {
    warn!("Failed to delete expired record {record_id}: {e}");
}
return Err(VaultError::ProofExpired);
```

## Affected Files

- `src/vault/proof_vault.rs` — one-line change in `retrieve_proof`

## Notes

- This is the smallest change in the backlog. A good first contribution or warm-up task.
- If migrating to `tracing` (IMP-009), use `tracing::warn!` instead of `log::warn!`.
