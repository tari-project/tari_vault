use std::future::Future;

use crate::{domain::StoredRecord, error::StorageError};

/// Abstraction over the persistence layer.
///
/// Implementations must be `Send + Sync` so they can be shared across async tasks.
/// All operations are single-record to keep the contract minimal and auditable.
pub trait StorageBackend: Send + Sync {
    /// Persist a new encrypted record under `record_id`.
    fn insert(
        &self,
        record_id: [u8; 16],
        record: StoredRecord,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Fetch a record by its `record_id`.
    ///
    /// Returns `StorageError::NotFound` when no matching record exists.
    fn fetch(
        &self,
        record_id: [u8; 16],
    ) -> impl Future<Output = Result<StoredRecord, StorageError>> + Send;

    /// Permanently delete a record, enforcing the single-use claim constraint.
    ///
    /// Silently succeeds if the record has already been removed.
    fn delete(&self, record_id: [u8; 16]) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Remove all records whose `expires_at` has passed.
    ///
    /// Returns the number of records deleted.
    fn delete_expired(&self) -> impl Future<Output = Result<usize, StorageError>> + Send;
}
