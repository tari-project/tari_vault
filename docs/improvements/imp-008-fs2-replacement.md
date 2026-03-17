# IMP-008: Replace Unmaintained `fs2` File-Lock Crate

**Status:** `[x]` Completed
**Tier:** 4 — Dependency Hygiene
**Priority:** Low (superseded by IMP-003 if SQLite backend is adopted)

## Problem

`fs2 = "0.4"` had not had a release since 2018. It was used exclusively in `src/storage/local_file.rs` for inter-process exclusive file locking to protect the vault JSON file against concurrent writes from multiple vault processes.

## Resolution

**Option A was chosen: migrate to `fd-lock`.**

`fd-lock 4.0` is an actively maintained cross-platform file-locking crate. The `LocalFileStore` inter-process lock was migrated from `fs2::FileExt::lock_exclusive` to `fd_lock::RwLock::write()`, with equivalent blocking semantics.

## What Changed

- `Cargo.toml`: removed `fs2`, added `fd-lock = "4.0.4"`
- `src/storage/local_file.rs`: replaced `fs2` lock usage with `fd_lock::RwLock`

## Affected Files

- `src/storage/local_file.rs`
- `Cargo.toml`
