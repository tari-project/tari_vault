use std::{sync::Arc, time::Duration};

use tokio::{task::JoinHandle, time};
use tokio_util::sync::CancellationToken;

use crate::vault::ProofVault;

/// Handle to a running background cleanup task.
///
/// Dropping this without calling `stop()` + `stopped()` will leave the task
/// running until the parent `CancellationToken` (if any) is cancelled.
pub struct CleanupTask {
    /// Child token — cancelling it stops only this task without affecting the
    /// parent token held by the host application.
    cancel: CancellationToken,
    handle: JoinHandle<()>,
}

impl CleanupTask {
    /// Signal the background sweep loop to exit after the current tick.
    ///
    /// Returns immediately.  Call [`stopped`](Self::stopped) to await full
    /// shutdown.
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    /// Wait for the background task to finish.
    ///
    /// Typically called after [`stop`](Self::stop) as part of an ordered
    /// application shutdown sequence.
    pub async fn stopped(self) {
        let _ = self.handle.await;
    }
}

/// Spawn a periodic background task that deletes expired proofs.
///
/// The sweep runs immediately on start (clearing leftover expired proofs from
/// any previous session), then repeats every `interval`.
///
/// # Cancellation
///
/// The task respects `parent_cancel`.  When either:
/// - `parent_cancel` is cancelled (host application shutdown), or
/// - [`CleanupTask::stop`] is called (targeted task shutdown),
///
/// …the loop exits cleanly after completing any in-progress sweep.
/// A *child token* is used internally so stopping the cleanup task does **not**
/// cancel the parent token.
///
/// # Embedding in walletd
///
/// ```rust,no_run
/// # use std::{sync::Arc, time::Duration};
/// # use tokio_util::sync::CancellationToken;
/// # use tari_vault::{
/// #     vault::{StandardVault, spawn_cleanup_task},
/// #     storage::LocalFileStore,
/// # };
/// # async fn example() -> anyhow::Result<()> {
/// let vault = Arc::new(StandardVault::new(LocalFileStore::new("vault.json".into())?));
///
/// // Reuse the host app's shutdown token so the sweep stops automatically.
/// let shutdown = CancellationToken::new();
/// let cleanup = spawn_cleanup_task(Arc::clone(&vault), Duration::from_secs(300), shutdown.clone());
///
/// // … on shutdown:
/// shutdown.cancel();          // or: cleanup.stop();
/// cleanup.stopped().await;
/// # Ok(()) }
/// ```
pub fn spawn_cleanup_task<V>(
    vault: Arc<V>,
    interval: Duration,
    parent_cancel: CancellationToken,
) -> CleanupTask
where
    V: ProofVault + 'static,
{
    // A child token lets us stop the task independently without cancelling the
    // parent (e.g. the host application's global shutdown token).
    let child = parent_cancel.child_token();
    let task_token = child.clone();

    let handle = tokio::spawn(async move {
        let mut ticker = time::interval(interval);
        // Skip a tick if the sweep takes longer than the interval rather than
        // letting ticks pile up.
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        tracing::debug!(
            target: "tari_vault::cleanup",
            "Cleanup task started (interval: {}s)",
            interval.as_secs()
        );

        loop {
            tokio::select! {
                // Cancellation has higher priority — always checked first.
                biased;

                _ = task_token.cancelled() => {
                    tracing::debug!(target: "tari_vault::cleanup", "Cleanup task stopping");
                    break;
                }

                _ = ticker.tick() => {
                    match vault.cleanup().await {
                        Ok(0) => {
                            tracing::debug!(target: "tari_vault::cleanup", "Periodic sweep: no expired proofs");
                        }
                        Ok(n) => {
                            tracing::info!(
                                target: "tari_vault::cleanup",
                                "Periodic sweep removed {n} expired proof(s)"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "tari_vault::cleanup",
                                "Cleanup sweep failed: {e}"
                            );
                        }
                    }
                }
            }
        }

        tracing::debug!(target: "tari_vault::cleanup", "Cleanup task stopped");
    });

    CleanupTask {
        cancel: child,
        handle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{EncryptedRecord, PlaintextProof, StoredRecord},
        storage::{LocalFileStore, StorageBackend},
        vault::{ProofVault, StandardVault},
    };
    use chrono::Utc;
    use tempfile::TempDir;

    fn vault_in(dir: &TempDir) -> Arc<StandardVault<LocalFileStore>> {
        Arc::new(StandardVault::new(
            LocalFileStore::new(dir.path().join("vault.json")).unwrap(),
        ))
    }

    /// Insert a record that is already expired directly into storage,
    /// bypassing `store_proof` (which now rejects TTL=0).
    async fn insert_expired_record(vault: &Arc<StandardVault<LocalFileStore>>) -> [u8; 16] {
        let record_id = [1u8; 16];
        let past = Utc::now() - chrono::Duration::seconds(1);
        let record = StoredRecord {
            encrypted: EncryptedRecord {
                nonce: vec![0u8; 12],
                ciphertext: vec![0u8; 16],
            },
            stored_at: past,
            expires_at: Some(past),
        };
        vault.storage.insert(record_id, record).await.unwrap();
        record_id
    }

    #[tokio::test]
    async fn cleanup_removes_expired_proofs() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);

        // Insert an already-expired record directly (bypassing store_proof).
        insert_expired_record(&vault).await;
        // Store a valid proof via the normal path.
        let valid = vault
            .store_proof(PlaintextProof::from_bytes(b"new".to_vec()), Some(3600))
            .await
            .unwrap();

        let removed = vault.cleanup().await.unwrap();
        assert_eq!(removed, 1, "only the expired proof should be removed");

        // Valid proof must still be retrievable.
        assert!(vault.retrieve_proof(valid.into()).await.is_ok());
    }

    #[tokio::test]
    async fn spawn_cleanup_task_runs_and_cancels() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);

        // Insert an already-expired record directly (bypassing store_proof).
        insert_expired_record(&vault).await;

        let cancel = CancellationToken::new();
        // Very short interval so the first sweep fires quickly.
        let task = spawn_cleanup_task(
            Arc::clone(&vault),
            Duration::from_millis(50),
            cancel.clone(),
        );

        // Allow time for at least one sweep.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Stop gracefully.
        task.stop();
        task.stopped().await;

        // The sweep must have cleaned the storage.
        let removed = vault.cleanup().await.unwrap();
        assert_eq!(removed, 0, "background task should have already swept");
    }

    #[tokio::test]
    async fn parent_cancel_stops_the_task() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);

        let parent = CancellationToken::new();
        let task = spawn_cleanup_task(Arc::clone(&vault), Duration::from_secs(60), parent.clone());

        // Cancelling the parent must propagate to the child task.
        parent.cancel();
        // stopped() must resolve (no deadlock).
        tokio::time::timeout(Duration::from_secs(2), task.stopped())
            .await
            .expect("task should stop within 2 s when parent token is cancelled");
    }
}
