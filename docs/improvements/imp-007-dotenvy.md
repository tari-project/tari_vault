# IMP-007: Replace `dotenv` with `dotenvy`

**Status:** `[ ]` Planned
**Tier:** 4 — Dependency Hygiene
**Priority:** Low

## Problem

The project depends on `dotenv = "0.15.0"`, which has been unmaintained since 2021. `dotenvy` is the community-maintained successor with an identical API, active maintenance, and published security advisories tracking.

## Goal

Replace the unmaintained `dotenv` crate with `dotenvy` at no behavioral cost.

## Change

`Cargo.toml`:
```toml
# Remove:
dotenv = "0.15.0"

# Add:
dotenvy = "0.15"
```

`src/config.rs`:
```rust
// Before:
use dotenv::dotenv;

// After:
use dotenvy::dotenv;
```

The `dotenv()` function signature is identical in both crates.

## Affected Files

- `Cargo.toml`
- `src/config.rs`

## Effort

Approximately 5 minutes. No behavioral change.
