# IMP-015: Per-Caller Rate Limiting on `storeProof`

**Status:** `[ ]` Planned
**Tier:** 6 — Minor / Nice-to-Have
**Priority:** Low

## Problem

There is no throttle on how many `vault_storeProof` calls an authenticated caller can make in a given time window. A single authenticated client (or a compromised token) can:

- Fill disk with stored proofs.
- Create a large in-memory workload during a `CleanupTask` sweep (O(N) with `LocalFileStore`).
- Exhaust ephemeral entropy pools if generating UUIDs and AES keys at high rate (unlikely in practice).

## Goal

Limit the rate of `storeProof` calls to a configurable threshold (e.g., requests per second or per minute).

## Proposed Approach

Add a Tower rate-limit layer to the HTTP middleware chain, or use the `governor` crate for a token-bucket rate limiter:

```toml
# Cargo.toml
governor = "0.6"
```

```rust
// In server.rs
let limiter = Arc::new(RateLimiter::direct(Quota::per_second(nonzero!(100u32))));
```

Since the current architecture uses a single shared Bearer token (not per-user tokens), rate limiting would apply globally to the vault endpoint, not per identity.

## Configuration

```toml
[server]
max_store_requests_per_second = 100  # 0 = disabled (default)
```

## Notes

- Rate limiting is most useful if the vault is exposed to a broader network. For a localhost-only deployment, this is low priority.
- If TLS (IMP-001) is implemented and multiple clients with distinct tokens are considered, per-token rate limiting becomes more meaningful and would require a token-keyed limiter map.
- `tower-governor` integrates `governor` directly into the Tower middleware stack.

## Affected Files

- `src/rpc/server.rs`
- `src/config.rs`
- `Cargo.toml`
