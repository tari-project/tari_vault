# IMP-003: SQLite Storage Backend

**Status:** `[ ]` Planned
**Tier:** 2 — Architecture / Storage
**Priority:** High

## Problem

`LocalFileStore` reads the entire vault JSON file, mutates it in memory, and writes the whole file back on every `insert`, `fetch`, `delete`, and `delete_expired` call. This is O(N) in the number of stored records for every operation. It also:

- Holds a large in-memory copy of all stored records during writes.
- Relies on `fs2` for inter-process locking, which is unmaintained (last release 2018).
- Cannot atomically combine `fetch` + `delete` in a single transaction (the current retrieve path has a window between decryption and deletion).
- Has no built-in upper bound on file size or record count.

## Goal

Provide an alternative `StorageBackend` implementation backed by SQLite that is O(1) per operation, atomic at the fetch+delete level, and does not require a separate file-lock crate.

## Proposed Design

### Schema

```sql
CREATE TABLE IF NOT EXISTS proofs (
    record_id   TEXT NOT NULL PRIMARY KEY,  -- hyphenated UUID
    nonce       TEXT NOT NULL,              -- base64-encoded, 12 bytes
    ciphertext  TEXT NOT NULL,              -- base64-encoded
    stored_at   TEXT NOT NULL,              -- ISO 8601 UTC
    expires_at  TEXT                        -- ISO 8601 UTC, NULL = no expiry
);

CREATE INDEX IF NOT EXISTS idx_expires_at ON proofs (expires_at)
    WHERE expires_at IS NOT NULL;
```

### Key Operations

- **`insert`** — `INSERT OR REPLACE INTO proofs ...`
- **`fetch`** — `SELECT ... FROM proofs WHERE record_id = ?`
- **`delete`** — `DELETE FROM proofs WHERE record_id = ?; RETURNING *` (detects absence)
- **`delete_expired`** — `DELETE FROM proofs WHERE expires_at IS NOT NULL AND expires_at < ?`
- **Atomic retrieve** — `BEGIN IMMEDIATE; SELECT ...; DELETE ...; COMMIT` (closes the fetch+delete race in `retrieve_proof`)

### Configuration

Add a `storage` section to `VaultConfig`:

```toml
[storage]
backend = "sqlite"          # or "file" (default)
sqlite_path = "/var/lib/tari_vault/vault.db"
```

### Crate

Use `sqlx` with the `sqlite` feature:
- Async, compile-time query verification (`sqlx::query!`).
- Connection pool handles concurrent access without a separate process-level mutex.
- Removes the need for `fs2` and the tokio `Mutex` wrapping `LocalFileStore`.

## Benefits Over `LocalFileStore`

| Property | `LocalFileStore` | `SqliteStore` |
|----------|-----------------|---------------|
| Per-operation complexity | O(N) | O(1) (indexed) |
| Atomic fetch+delete | No | Yes (single transaction) |
| Inter-process locking | `fs2` (unmaintained) | SQLite WAL mode (built-in) |
| TTL sweep | Full file scan | Index-range delete |
| Max size enforcement | None | Configurable `PRAGMA max_page_count` |
| Cross-platform | Partial (0600 perms Unix only) | Full |

## Affected Files

- `src/storage/sqlite.rs` — new `SqliteStore` implementation
- `src/storage/mod.rs` — export `SqliteStore`
- `src/config.rs` — `StorageConfig` enum (`File` | `Sqlite`)
- `src/main.rs` — backend selection at startup
- `Cargo.toml` — `sqlx` with `sqlite`, `runtime-tokio`, `macros` features
- `Makefile` — optional: add `make migrate` target

## Notes

- `LocalFileStore` is retained as the default for zero-dependency deployments.
- SQLite database file should have `0600` permissions set at creation, matching the current file store behavior.
- WAL mode (`PRAGMA journal_mode=WAL`) is recommended for concurrent read performance.
- Removing `fs2` is a separate task (IMP-008) but naturally follows from adopting this backend.
