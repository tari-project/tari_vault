# IMP-014: Fix Lossy `u64 → i64` Cast for `expires_in_secs`

**Status:** `[ ]` Planned
**Tier:** 6 — Minor / Nice-to-Have
**Priority:** Low

## Problem

In `StandardVault::store_proof` (`src/vault/proof_vault.rs`):

```rust
expires_in_secs.map(|s| Utc::now() + Duration::seconds(s as i64))
```

The `s as i64` cast silently truncates any `u64` value greater than `i64::MAX` (approximately 9.2 × 10¹⁸ seconds, or ~292 billion years). While this overflow is entirely theoretical in practice, `as` casts are flaggable by Clippy (`clippy::cast_possible_truncation`) and represent a code quality issue.

## Goal

Replace the lossy cast with a checked conversion that produces a clear error for out-of-range values.

## Proposed Fix

```rust
expires_in_secs
    .map(|s| {
        let secs = i64::try_from(s)
            .map_err(|_| VaultError::InvalidParameter("expires_in_secs value is too large"))?;
        Ok(Utc::now() + Duration::seconds(secs))
    })
    .transpose()?
```

Alternatively, if a `VaultError::InvalidParameter` variant does not exist (see IMP-005), cap at a reasonable maximum (e.g., 10 years in seconds: `315_360_000u64`).

## Affected Files

- `src/vault/proof_vault.rs` — one change in `store_proof`
- `src/error.rs` — possibly add `VaultError::InvalidParameter` if not added by IMP-005

## Notes

- This is a 2-line change. Combine with IMP-005 since both require `VaultError::InvalidParameter`.
