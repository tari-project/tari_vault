# IMP-008: Replace Unmaintained `fs2` File-Lock Crate

**Status:** `[ ]` Planned
**Tier:** 4 — Dependency Hygiene
**Priority:** Low (superseded by IMP-003 if SQLite backend is adopted)

## Problem

`fs2 = "0.4"` has not had a release since 2018. It is used exclusively in `src/storage/local_file.rs` for inter-process exclusive file locking to protect the vault JSON file against concurrent writes from multiple vault processes.

## Goal

Replace `fs2` with an actively maintained alternative, or eliminate the dependency entirely by adopting the SQLite backend (IMP-003).

## Options

### Option A: Migrate to `fd-lock` (recommended if keeping `LocalFileStore`)

`fd-lock` is an actively maintained cross-platform file-locking crate. API is slightly different but the concept is identical.

### Option B: Migrate to `file-lock`

Another maintained option with a simpler API surface.

### Option C: Remove via IMP-003 (preferred)

If the SQLite backend is implemented, `LocalFileStore` becomes the legacy/simple backend and can retain `fs2` in a deprecated state, or the SQLite path eliminates the need for `LocalFileStore` entirely in production deployments. `fs2` is removed as a consequence.

## Recommendation

Defer this task. If IMP-003 (SQLite backend) is implemented, evaluate whether `LocalFileStore` will remain in the codebase long-term. If yes, migrate to `fd-lock`. If `LocalFileStore` is deprecated, close this issue as resolved by IMP-003.

## Affected Files

- `src/storage/local_file.rs`
- `Cargo.toml`
