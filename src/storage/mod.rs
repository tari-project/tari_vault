pub mod backend;
pub mod local_file;
pub mod sqlite;

pub use backend::StorageBackend;
pub use local_file::LocalFileStore;
pub use sqlite::SqliteStore;

use crate::{domain::StoredRecord, error::StorageError};

/// A type-erased storage backend that dispatches to either the file or SQLite
/// implementation at runtime.
///
/// This enum exists to satisfy the `StorageBackend: Sized` requirement of
/// `StandardVault<B>` while keeping `main.rs` free of generics — a single
/// `StandardVault<AnyBackend>` is constructed regardless of which backend is
/// active.
pub enum AnyBackend {
    File(LocalFileStore),
    Sqlite(SqliteStore),
}

impl StorageBackend for AnyBackend {
    async fn insert(&self, record_id: [u8; 16], record: StoredRecord) -> Result<(), StorageError> {
        match self {
            AnyBackend::File(s) => s.insert(record_id, record).await,
            AnyBackend::Sqlite(s) => s.insert(record_id, record).await,
        }
    }

    async fn fetch(&self, record_id: [u8; 16]) -> Result<StoredRecord, StorageError> {
        match self {
            AnyBackend::File(s) => s.fetch(record_id).await,
            AnyBackend::Sqlite(s) => s.fetch(record_id).await,
        }
    }

    async fn delete(&self, record_id: [u8; 16]) -> Result<(), StorageError> {
        match self {
            AnyBackend::File(s) => s.delete(record_id).await,
            AnyBackend::Sqlite(s) => s.delete(record_id).await,
        }
    }

    async fn delete_expired(&self) -> Result<usize, StorageError> {
        match self {
            AnyBackend::File(s) => s.delete_expired().await,
            AnyBackend::Sqlite(s) => s.delete_expired().await,
        }
    }
}
