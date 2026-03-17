use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use rusqlite_migration::{M, Migrations};

use crate::{
    domain::{EncryptedRecord, StoredRecord},
    error::StorageError,
    storage::backend::StorageBackend,
};

static MIGRATIONS: std::sync::LazyLock<Migrations<'static>> = std::sync::LazyLock::new(|| {
    Migrations::new(vec![M::up(
        "CREATE TABLE IF NOT EXISTS proofs (
                record_id   BLOB NOT NULL PRIMARY KEY,
                nonce       BLOB NOT NULL,
                ciphertext  BLOB NOT NULL,
                stored_at   TEXT NOT NULL,
                expires_at  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_expires_at ON proofs (expires_at)
                WHERE expires_at IS NOT NULL;",
    )])
});

/// SQLite-backed storage implementation.
///
/// # Durability & Safety
/// - WAL mode enables concurrent reads without blocking writes.
/// - A `std::sync::Mutex` serialises all database operations within this process.
/// - All calls are offloaded to `tokio::task::spawn_blocking` to avoid blocking
///   the async executor.
/// - The database file is created with `0600` permissions on Unix.
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open (or create) the SQLite database at `db_path`.
    pub fn new(db_path: PathBuf) -> Result<Self, StorageError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut conn = Connection::open(&db_path).map_err(db_error)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA secure_delete = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(db_error)?;

        MIGRATIONS.to_latest(&mut conn).map_err(|e| {
            StorageError::Io(std::io::Error::other(format!("Migration failed: {e}")))
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

// ── StorageBackend impl ──────────────────────────────────────────────────────

impl StorageBackend for SqliteStore {
    async fn insert(&self, record_id: [u8; 16], record: StoredRecord) -> Result<(), StorageError> {
        let conn = Arc::clone(&self.conn);
        let key = record_id.to_vec();
        let nonce = record.encrypted.nonce;
        let ciphertext = record.encrypted.ciphertext;
        let stored_at = record.stored_at.to_rfc3339();
        let expires_at = record.expires_at.map(|t| t.to_rfc3339());

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(mutex_error)?;
            conn.execute(
                "INSERT OR REPLACE INTO proofs (record_id, nonce, ciphertext, stored_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![key, nonce, ciphertext, stored_at, expires_at],
            )
            .map_err(db_error)?;
            Ok::<_, StorageError>(())
        })
        .await
        .map_err(join_error)??;

        Ok(())
    }

    async fn fetch(&self, record_id: [u8; 16]) -> Result<StoredRecord, StorageError> {
        let conn = Arc::clone(&self.conn);
        let key = record_id.to_vec();

        let record = tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(mutex_error)?;
            let mut stmt = conn
                .prepare_cached(
                    "SELECT nonce, ciphertext, stored_at, expires_at
                     FROM proofs WHERE record_id = ?1",
                )
                .map_err(db_error)?;

            match stmt.query_row(params![key], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            }) {
                Ok((nonce, ciphertext, stored_at_str, expires_at_str)) => {
                    let stored_at = parse_datetime(&stored_at_str)?;
                    let expires_at = expires_at_str.map(|s| parse_datetime(&s)).transpose()?;
                    Ok(StoredRecord {
                        encrypted: EncryptedRecord { nonce, ciphertext },
                        stored_at,
                        expires_at,
                    })
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Err(StorageError::NotFound),
                Err(e) => Err(db_error(e)),
            }
        })
        .await
        .map_err(join_error)??;

        Ok(record)
    }

    async fn delete(&self, record_id: [u8; 16]) -> Result<(), StorageError> {
        let conn = Arc::clone(&self.conn);
        let key = record_id.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(mutex_error)?;
            conn.execute("DELETE FROM proofs WHERE record_id = ?1", params![key])
                .map_err(db_error)?;
            // Silently succeeds even when record_id was already absent.
            Ok::<_, StorageError>(())
        })
        .await
        .map_err(join_error)??;

        Ok(())
    }

    async fn delete_expired(&self) -> Result<usize, StorageError> {
        let conn = Arc::clone(&self.conn);
        let now = Utc::now().to_rfc3339();

        let removed = tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(mutex_error)?;
            let count = conn
                .execute(
                    "DELETE FROM proofs WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                    params![now],
                )
                .map_err(db_error)?;
            Ok::<usize, StorageError>(count)
        })
        .await
        .map_err(join_error)??;

        Ok(removed)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn db_error(e: rusqlite::Error) -> StorageError {
    StorageError::Io(std::io::Error::other(e.to_string()))
}

fn mutex_error<T>(e: std::sync::PoisonError<T>) -> StorageError {
    StorageError::Io(std::io::Error::other(format!("mutex poisoned: {e}")))
}

fn join_error(e: tokio::task::JoinError) -> StorageError {
    StorageError::Io(std::io::Error::other(e.to_string()))
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            StorageError::Io(std::io::Error::other(format!(
                "invalid datetime '{s}': {e}"
            )))
        })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::EncryptedRecord;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn make_record(expires_in_secs: Option<i64>) -> StoredRecord {
        StoredRecord {
            encrypted: EncryptedRecord {
                nonce: vec![0u8; 12],
                ciphertext: b"hello".to_vec(),
            },
            stored_at: Utc::now(),
            expires_at: expires_in_secs.map(|s| Utc::now() + chrono::Duration::seconds(s)),
        }
    }

    fn store_in(dir: &TempDir) -> SqliteStore {
        SqliteStore::new(dir.path().join("vault.db")).unwrap()
    }

    #[tokio::test]
    async fn insert_fetch_delete_round_trip() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        let id = *Uuid::new_v4().as_bytes();
        let record = make_record(None);

        store.insert(id, record.clone()).await.unwrap();
        let fetched = store.fetch(id).await.unwrap();
        assert_eq!(fetched.encrypted.ciphertext, b"hello");

        store.delete(id).await.unwrap();
        assert!(matches!(store.fetch(id).await, Err(StorageError::NotFound)));
    }

    #[tokio::test]
    async fn delete_expired_removes_stale_records() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);

        let id_expired = *Uuid::new_v4().as_bytes();
        let id_valid = *Uuid::new_v4().as_bytes();

        store
            .insert(id_expired, make_record(Some(-1)))
            .await
            .unwrap();
        store
            .insert(id_valid, make_record(Some(3600)))
            .await
            .unwrap();

        let removed = store.delete_expired().await.unwrap();
        assert_eq!(removed, 1);
        assert!(matches!(
            store.fetch(id_expired).await,
            Err(StorageError::NotFound)
        ));
        assert!(store.fetch(id_valid).await.is_ok());
    }

    #[tokio::test]
    async fn delete_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        let id = *Uuid::new_v4().as_bytes();

        // Deleting a non-existent record should succeed silently.
        assert!(store.delete(id).await.is_ok());
    }
}
