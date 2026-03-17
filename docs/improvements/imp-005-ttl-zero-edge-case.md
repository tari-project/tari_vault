# IMP-005: Harden TTL=0 Edge Case

**Status:** `[ ]` Planned
**Tier:** 3 — Reliability
**Priority:** Low–Medium

## Problem

Calling `vault_storeProof` with `expires_in_secs: 0` computes `expires_at = Utc::now() + Duration::seconds(0)`, which equals the current timestamp at the exact moment of storage. Whether the stored record appears expired on first access depends on sub-millisecond clock precision and scheduling latency.

The unit test at `src/vault/proof_vault.rs` works around this with an explicit `tokio::time::sleep(Duration::from_millis(1))`, acknowledging the race. A caller using TTL=0 in production would have undefined behavior — the proof may or may not be retrievable.

## Goal

Give TTL=0 a clear, deterministic semantic.

## Options

### Option A: Reject TTL=0 (recommended)

Return a validation error if `expires_in_secs` is `Some(0)`. TTL must be either absent (no expiry) or a positive integer.

```
VaultError::InvalidParameter("expires_in_secs must be greater than zero")
```

### Option B: Treat as "store but immediately mark expired"

Accept TTL=0, store the record, but return the `Claim_ID` with the knowledge that it is already expired. This is a well-defined semantic (useful for testing) but surprising in production use.

### Option C: Minimum TTL floor

Enforce a minimum of 1 second (`expires_in_secs.map(|s| s.max(1))`). Silent behavior change; not recommended.

**Recommendation: Option A.** It is the least surprising and easiest to document. Callers with legitimate zero-TTL needs are better served by calling `storeProof` + `deleteProof` in sequence.

## Affected Files

- `src/vault/proof_vault.rs` — validation in `store_proof`
- `src/error.rs` — potentially a new `VaultError::InvalidParameter` variant
- `openrpc.json` — document the constraint
- `tests/rpc_integration.rs` — add a test for the rejection case; update the TTL=0 unit test

## Notes

- The existing TTL=0 unit test (`test_proof_expires`) uses `expires_in_secs: Some(1)` plus a sleep, so it is not affected by this change.
- The problematic TTL=0 path appears in a separate edge-case test that can be converted to assert the new error.
