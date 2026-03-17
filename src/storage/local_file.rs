use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{domain::StoredRecord, error::StorageError, storage::backend::StorageBackend};

/// Disk-format: a JSON object keyed by hyphenated UUID strings.
type DiskState = BTreeMap<String, StoredRecord>;

/// File-backed storage implementation.
///
/// # Durability & Safety
/// - Every mutation writes to a temp file first, then atomically renames it over
///   the vault file.  A crash mid-write cannot corrupt existing data.
/// - An `fd-lock` exclusive lock on a `.lock` sidecar file serialises concurrent
///   access from *different processes*.
/// - A `tokio::sync::Mutex` serialises concurrent access within a single process
///   without blocking the async executor.
/// - The vault file is created with `0600` permissions on Unix so that only the
///   owning user can read it.
pub struct LocalFileStore {
    vault_path: PathBuf,
    /// Serialises all operations within this process.
    process_lock: Mutex<()>,
}

impl LocalFileStore {
    /// Open (or create) the vault file at `vault_path`.
    pub fn new(vault_path: PathBuf) -> Result<Self, StorageError> {
        if let Some(parent) = vault_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if !vault_path.exists() {
            let empty = DiskState::new();
            write_atomic(&vault_path, &empty)?;
        }

        Ok(Self {
            vault_path,
            process_lock: Mutex::new(()),
        })
    }

    // ── Private helpers (called inside spawn_blocking) ──────────────────────

    fn lock_path(vault_path: &Path) -> PathBuf {
        vault_path.with_extension("lock")
    }

    fn open_lock_file(vault_path: &Path) -> Result<fd_lock::RwLock<File>, StorageError> {
        let lock_path = Self::lock_path(vault_path);
        let f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        Ok(fd_lock::RwLock::new(f))
    }

    fn read_state(vault_path: &Path) -> Result<DiskState, StorageError> {
        let content = std::fs::read(vault_path)?;
        Ok(serde_json::from_slice(&content)?)
    }
}

// ── StorageBackend impl ──────────────────────────────────────────────────────

impl StorageBackend for LocalFileStore {
    async fn insert(&self, record_id: [u8; 16], record: StoredRecord) -> Result<(), StorageError> {
        let _guard = self.process_lock.lock().await;
        let vault_path = self.vault_path.clone();
        let key = Uuid::from_bytes(record_id).hyphenated().to_string();

        tokio::task::spawn_blocking(move || {
            let mut file_lock = Self::open_lock_file(&vault_path)?;
            let _guard = file_lock.write()?;
            let mut state = Self::read_state(&vault_path)?;
            state.insert(key, record);
            write_atomic(&vault_path, &state)
        })
        .await
        .map_err(join_error)??;

        Ok(())
    }

    async fn fetch(&self, record_id: [u8; 16]) -> Result<StoredRecord, StorageError> {
        let _guard = self.process_lock.lock().await;
        let vault_path = self.vault_path.clone();
        let key = Uuid::from_bytes(record_id).hyphenated().to_string();

        let record = tokio::task::spawn_blocking(move || {
            let mut file_lock = Self::open_lock_file(&vault_path)?;
            let _guard = file_lock.write()?;
            let state = Self::read_state(&vault_path)?;
            state.get(&key).cloned().ok_or(StorageError::NotFound)
        })
        .await
        .map_err(join_error)??;

        Ok(record)
    }

    async fn delete(&self, record_id: [u8; 16]) -> Result<bool, StorageError> {
        let _guard = self.process_lock.lock().await;
        let vault_path = self.vault_path.clone();
        let key = Uuid::from_bytes(record_id).hyphenated().to_string();

        let removed = tokio::task::spawn_blocking(move || {
            let mut file_lock = Self::open_lock_file(&vault_path)?;
            let _guard = file_lock.write()?;
            let mut state = Self::read_state(&vault_path)?;
            let existed = state.remove(&key).is_some();
            if existed {
                write_atomic(&vault_path, &state)?;
            }
            Ok::<bool, StorageError>(existed)
        })
        .await
        .map_err(join_error)??;

        Ok(removed)
    }

    async fn delete_expired(&self) -> Result<usize, StorageError> {
        let _guard = self.process_lock.lock().await;
        let vault_path = self.vault_path.clone();
        let now = Utc::now();

        let removed = tokio::task::spawn_blocking(move || {
            let mut file_lock = Self::open_lock_file(&vault_path)?;
            let _guard = file_lock.write()?;
            let mut state = Self::read_state(&vault_path)?;
            let before = state.len();
            state.retain(|_, r| r.expires_at.map(|t| t > now).unwrap_or(true));
            let removed = before - state.len();
            if removed > 0 {
                write_atomic(&vault_path, &state)?;
            }
            Ok::<usize, StorageError>(removed)
        })
        .await
        .map_err(join_error)??;

        Ok(removed)
    }
}

// ── File utilities ───────────────────────────────────────────────────────────

/// Write `state` to disk atomically via a temp file + rename.
fn write_atomic(vault_path: &Path, state: &DiskState) -> Result<(), StorageError> {
    let parent = vault_path
        .parent()
        .ok_or_else(|| StorageError::Io(std::io::Error::other("vault path has no parent")))?;

    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let json = serde_json::to_vec_pretty(state)?;
    temp.write_all(&json)?;
    temp.flush()?;

    temp.persist(vault_path)
        .map_err(|e| StorageError::Io(e.error))?;

    // Restrict permissions to owner-only on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(vault_path, perms)?;
    }

    Ok(())
}

fn join_error(e: tokio::task::JoinError) -> StorageError {
    StorageError::Io(std::io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{EncryptedRecord, StoredRecord};
    use chrono::Utc;
    use tempfile::TempDir;

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

    fn store_in(dir: &TempDir) -> LocalFileStore {
        LocalFileStore::new(dir.path().join("vault.json")).unwrap()
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
}
