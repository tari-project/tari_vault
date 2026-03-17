# IMP-004: Collapse `delete_proof` to Single Storage Round-Trip

**Status:** `[x]` Completed
**Tier:** 2 — Architecture / Storage
**Priority:** Medium

## Problem

`StandardVault::delete_proof` (`src/vault/proof_vault.rs`) calls `storage.fetch()` to verify existence and then calls `storage.delete()` as two separate operations. Each operation acquires the tokio Mutex, spawns a blocking task, acquires the `fs2` file lock, reads the full JSON file, and writes it back. This is two full lock-acquire → read → write → release cycles for one logical "remove and return whether it existed" operation.

Additionally, `StorageBackend::delete()` currently returns `Result<(), StorageError>` and silently ignores missing records, making it impossible to distinguish "deleted" from "was never there" at the vault level.

## Goal

Reduce `delete_proof` to a single storage operation. Surface whether the record existed so `delete_proof` can return a meaningful `ProofNotFound` error when appropriate.

## Proposed Fix

Change the `StorageBackend::delete` signature to return a boolean indicating whether a record was actually removed:

```rust
// Before
async fn delete(&self, record_id: Uuid) -> Result<(), StorageError>;

// After
async fn delete(&self, record_id: Uuid) -> Result<bool, StorageError>;
// Returns Ok(true) if deleted, Ok(false) if not found
```

`StandardVault::delete_proof` then becomes:

```rust
let claim_id = ClaimId::decode(&claim_id_str)?;
let deleted = self.storage.delete(claim_id.record_id()).await?;
if !deleted {
    return Err(VaultError::ProofNotFound);
}
Ok(())
```

The TTL cleanup path in `CleanupTask` calls `storage.delete_expired()` (unaffected). The `retrieve_proof` path calls `storage.delete()` after a successful decrypt — it can ignore the boolean (the record was just fetched, so it must exist).

## Affected Files

- `src/storage/backend.rs` — trait signature change
- `src/storage/local_file.rs` — return `Ok(state.proofs.remove(...).is_some())`
- `src/vault/proof_vault.rs` — `delete_proof` implementation
- `tests/rpc_integration.rs` — no expected behavioral changes; tests should continue to pass

## Notes

- This change also benefits the SQLite backend (IMP-003): `DELETE ... RETURNING *` or checking `rows_affected()` is natural in SQL.
- The `retrieve_proof` path ignores the boolean safely — a `false` there would indicate a concurrent deletion race, which is already handled by the `fetch` earlier in the same lock scope.
