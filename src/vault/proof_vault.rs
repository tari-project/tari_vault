use std::{future::Future, sync::Arc};

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use chrono::{Duration, Utc};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    domain::{ClaimId, EncryptedRecord, PlaintextProof, StoredRecord},
    error::{StorageError, VaultError},
    storage::StorageBackend,
};

/// Primary interface used by both the Sender and the Receiver.
pub trait ProofVault: Send + Sync {
    /// Encrypt `proof`, persist it, and return a `Claim_ID` string.
    ///
    /// `expires_in_secs`: optional TTL.  `None` means the proof never expires.
    fn store_proof(
        &self,
        proof: PlaintextProof,
        expires_in_secs: Option<u64>,
    ) -> impl Future<Output = Result<String, VaultError>> + Send;

    /// Decode `claim_id`, decrypt the associated proof, delete it from storage
    /// (single-use), and return the plaintext.
    fn retrieve_proof(
        &self,
        claim_id_str: Zeroizing<String>,
    ) -> impl Future<Output = Result<PlaintextProof, VaultError>> + Send;

    /// Delete all proofs whose TTL has elapsed and return the count removed.
    ///
    /// Safe to call at any time — proofs without a TTL are never affected.
    /// Exposed so host applications can trigger an on-demand sweep (e.g. at
    /// startup) in addition to, or instead of, the periodic background task.
    fn cleanup(&self) -> impl Future<Output = Result<usize, VaultError>> + Send;

    /// Explicitly delete a stored proof without decrypting it.
    ///
    /// This is the **abort / cancel** path: the holder of a `Claim_ID` can
    /// discard the proof when the flow is abandoned (e.g. the L1 burn is
    /// rolled back, the AI agent encounters an error, or the user cancels).
    ///
    /// Requires the full `Claim_ID` token — only the entity that received
    /// the token from the sender can delete it, preventing unauthorised
    /// deletion by a party who only knows the `Record_ID`.
    ///
    /// Returns `VaultError::ProofNotFound` if the proof has already been
    /// retrieved, deleted, or never existed — giving the caller explicit
    /// feedback rather than silently succeeding.
    fn delete_proof(
        &self,
        claim_id_str: Zeroizing<String>,
    ) -> impl Future<Output = Result<(), VaultError>> + Send;
}

/// Reference implementation backed by any `StorageBackend`.
pub struct StandardVault<B> {
    pub(crate) storage: Arc<B>,
}

impl<B: StorageBackend> StandardVault<B> {
    pub fn new(storage: B) -> Self {
        Self {
            storage: Arc::new(storage),
        }
    }
}

impl<B: StorageBackend + 'static> ProofVault for StandardVault<B> {
    async fn store_proof(
        &self,
        proof: PlaintextProof,
        expires_in_secs: Option<u64>,
    ) -> Result<String, VaultError> {
        if expires_in_secs == Some(0) {
            return Err(VaultError::InvalidParameter(
                "expires_in_secs must be greater than zero".into(),
            ));
        }

        let generated_key = Aes256Gcm::generate_key(OsRng);
        let nonce = Aes256Gcm::generate_nonce(OsRng);

        let cipher = Aes256Gcm::new(&generated_key);
        let ciphertext = cipher
            .encrypt(&nonce, proof.as_bytes())
            .map_err(|_| VaultError::DecryptionFailed)?;
        // `proof` is dropped here — ZeroizeOnDrop wipes the plaintext.
        drop(proof);

        let mut key_bytes = Zeroizing::new([0u8; 32]);
        key_bytes.copy_from_slice(&generated_key);
        // GenericArray is Copy so drop() is a no-op — explicitly zeroize.
        let mut gk = generated_key;
        gk.zeroize();

        let claim_id = ClaimId::new(*key_bytes);
        // `key_bytes` is dropped — Zeroizing wipes the stack copy.

        let record_id = claim_id.record_id;

        let expires_at = expires_in_secs
            .map(|s| -> Result<_, VaultError> {
                let secs = i64::try_from(s).map_err(|_| {
                    VaultError::InvalidParameter("expires_in_secs value is too large".into())
                })?;
                Ok(Utc::now() + Duration::seconds(secs))
            })
            .transpose()?;

        let stored = StoredRecord {
            encrypted: EncryptedRecord {
                nonce: nonce.to_vec(),
                ciphertext,
            },
            stored_at: Utc::now(),
            expires_at,
        };

        self.storage.insert(record_id, stored).await?;

        let encoded = claim_id.encode();
        // `claim_id` is dropped here — ZeroizeOnDrop wipes encryption_key.
        tracing::info!(
            target: "tari_vault::vault",
            "Proof stored; record_id={}",
            uuid::Uuid::from_bytes(record_id).simple()
        );

        Ok(encoded)
    }

    async fn retrieve_proof(
        &self,
        claim_id_str: Zeroizing<String>,
    ) -> Result<PlaintextProof, VaultError> {
        let claim_id = ClaimId::decode(&claim_id_str)?;
        // Zeroizing<String> overwrites the heap allocation on drop.
        drop(claim_id_str);

        let record_id = claim_id.record_id;

        let stored = self.storage.fetch(record_id).await.map_err(|e| match e {
            StorageError::NotFound => VaultError::ProofNotFound,
            other => VaultError::Storage(other),
        })?;

        if stored.is_expired() {
            if let Err(e) = self.storage.delete(record_id).await {
                tracing::warn!(
                    target: "tari_vault::vault",
                    "Failed to delete expired record {}; storage error: {e}",
                    uuid::Uuid::from_bytes(record_id).simple()
                );
            }
            tracing::warn!(
                target: "tari_vault::vault",
                "Expired proof access attempt; record_id={}",
                uuid::Uuid::from_bytes(record_id).simple()
            );
            return Err(VaultError::ProofExpired);
        }

        if stored.encrypted.nonce.len() != 12 {
            return Err(VaultError::DecryptionFailed);
        }

        let key = Key::<Aes256Gcm>::from_slice(&claim_id.encryption_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&stored.encrypted.nonce);

        let plaintext = cipher
            .decrypt(nonce, stored.encrypted.ciphertext.as_ref())
            // Generic error — never reveal why decryption failed.
            .map_err(|_| VaultError::DecryptionFailed)?;

        self.storage.delete(record_id).await?;

        // `claim_id` is dropped here — ZeroizeOnDrop wipes encryption_key.
        tracing::info!(
            target: "tari_vault::vault",
            "Proof retrieved and consumed; record_id={}",
            uuid::Uuid::from_bytes(record_id).simple()
        );

        Ok(PlaintextProof::from_bytes(plaintext))
    }

    async fn cleanup(&self) -> Result<usize, VaultError> {
        let removed = self.storage.delete_expired().await?;
        if removed > 0 {
            tracing::info!(
                target: "tari_vault::vault",
                "Cleanup sweep removed {removed} expired proof(s)"
            );
        }
        Ok(removed)
    }

    async fn delete_proof(&self, claim_id_str: Zeroizing<String>) -> Result<(), VaultError> {
        let claim_id = ClaimId::decode(&claim_id_str)?;
        // Zeroizing<String> overwrites the heap allocation on drop.
        drop(claim_id_str);

        let record_id = claim_id.record_id;

        let stored = self.storage.fetch(record_id).await.map_err(|e| match e {
            StorageError::NotFound => VaultError::ProofNotFound,
            other => VaultError::Storage(other),
        })?;

        if stored.is_expired() {
            if let Err(e) = self.storage.delete(record_id).await {
                tracing::warn!(
                    target: "tari_vault::vault",
                    "Failed to delete expired record {}; storage error: {e}",
                    uuid::Uuid::from_bytes(record_id).simple()
                );
            }
            tracing::warn!(
                target: "tari_vault::vault",
                "Delete attempt on expired proof; record_id={}",
                uuid::Uuid::from_bytes(record_id).simple()
            );
            return Err(VaultError::ProofExpired);
        }

        let deleted = self.storage.delete(record_id).await?;
        if !deleted {
            return Err(VaultError::ProofNotFound);
        }

        // `claim_id` is dropped here — ZeroizeOnDrop wipes encryption_key.
        tracing::info!(
            target: "tari_vault::vault",
            "Proof explicitly deleted by holder; record_id={}",
            uuid::Uuid::from_bytes(record_id).simple()
        );

        Ok(())
    }
}

// ── Arc<V> blanket impl ──────────────────────────────────────────────────────
//
// Lets callers share a single Arc-wrapped vault between the RPC server and the
// cleanup task without needing to clone the underlying storage.

impl<V: ProofVault + Send + Sync> ProofVault for Arc<V> {
    async fn store_proof(
        &self,
        proof: PlaintextProof,
        expires_in_secs: Option<u64>,
    ) -> Result<String, VaultError> {
        (**self).store_proof(proof, expires_in_secs).await
    }

    async fn retrieve_proof(
        &self,
        claim_id_str: Zeroizing<String>,
    ) -> Result<PlaintextProof, VaultError> {
        (**self).retrieve_proof(claim_id_str).await
    }

    async fn cleanup(&self) -> Result<usize, VaultError> {
        (**self).cleanup().await
    }

    async fn delete_proof(&self, claim_id_str: Zeroizing<String>) -> Result<(), VaultError> {
        (**self).delete_proof(claim_id_str).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalFileStore;
    use serde_json::json;
    use tempfile::TempDir;

    fn vault_in(dir: &TempDir) -> StandardVault<LocalFileStore> {
        let store = LocalFileStore::new(dir.path().join("vault.json")).unwrap();
        StandardVault::new(store)
    }

    #[tokio::test]
    async fn store_and_retrieve_json_proof() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);
        let proof_value = json!({"root": "abc123", "path": [1, 2, 3]});
        let proof = PlaintextProof::from_json(&proof_value).unwrap();

        let claim_id = vault.store_proof(proof, None).await.unwrap();
        assert_eq!(claim_id.len(), 64); // 48 raw bytes → 64-char base64url

        let retrieved = vault
            .retrieve_proof(Zeroizing::new(claim_id))
            .await
            .unwrap();
        let retrieved_json = retrieved.into_json().unwrap();
        assert_eq!(retrieved_json, proof_value);
    }

    #[tokio::test]
    async fn single_use_enforced() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);
        let proof = PlaintextProof::from_bytes(b"secret".to_vec());

        let claim_id = vault.store_proof(proof, None).await.unwrap();
        vault
            .retrieve_proof(Zeroizing::new(claim_id.clone()))
            .await
            .unwrap();

        // Second retrieval must fail.
        let err = vault
            .retrieve_proof(Zeroizing::new(claim_id))
            .await
            .unwrap_err();
        assert!(matches!(err, VaultError::ProofNotFound));
    }

    #[tokio::test]
    async fn store_proof_with_zero_ttl_returns_invalid_parameter() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);
        let proof = PlaintextProof::from_bytes(b"expires".to_vec());

        let err = vault.store_proof(proof, Some(0)).await.unwrap_err();
        assert!(matches!(err, VaultError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn expired_proof_returns_error() {
        use crate::storage::StorageBackend;
        use chrono::Utc;

        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);

        // Store with a normal TTL, then overwrite the record in storage with a
        // past expiry — avoids sleeping in tests.
        let claim_id = vault
            .store_proof(PlaintextProof::from_bytes(b"expires".to_vec()), Some(3600))
            .await
            .unwrap();

        let decoded = ClaimId::decode(&Zeroizing::new(claim_id.clone())).unwrap();
        let record_id = decoded.record_id;
        let mut record = vault.storage.fetch(record_id).await.unwrap();
        record.expires_at = Some(Utc::now() - Duration::seconds(1));
        vault.storage.insert(record_id, record).await.unwrap();

        let err = vault
            .retrieve_proof(Zeroizing::new(claim_id))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            VaultError::ProofExpired | VaultError::ProofNotFound
        ));
    }

    #[tokio::test]
    async fn delete_proof_removes_it() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);
        let proof = PlaintextProof::from_bytes(b"abort-me".to_vec());

        let claim_id = vault.store_proof(proof, None).await.unwrap();

        // Explicit delete must succeed.
        vault
            .delete_proof(Zeroizing::new(claim_id.clone()))
            .await
            .unwrap();

        // Subsequent retrieval must fail with ProofNotFound.
        let err = vault
            .retrieve_proof(Zeroizing::new(claim_id))
            .await
            .unwrap_err();
        assert!(matches!(err, VaultError::ProofNotFound));
    }

    #[tokio::test]
    async fn delete_proof_returns_not_found_when_already_consumed() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);
        let proof = PlaintextProof::from_bytes(b"consume-then-delete".to_vec());

        let claim_id = vault.store_proof(proof, None).await.unwrap();
        vault
            .retrieve_proof(Zeroizing::new(claim_id.clone()))
            .await
            .unwrap();

        // After retrieval the record is gone — delete must return ProofNotFound.
        let err = vault
            .delete_proof(Zeroizing::new(claim_id))
            .await
            .unwrap_err();
        assert!(matches!(err, VaultError::ProofNotFound));
    }

    #[tokio::test]
    async fn wrong_claim_id_fails_decryption() {
        let dir = TempDir::new().unwrap();
        let vault = vault_in(&dir);
        let proof = PlaintextProof::from_bytes(b"secret".to_vec());

        let claim_id = vault.store_proof(proof, None).await.unwrap();

        // Corrupt the key portion by flipping a single base64url character
        // in the key half (positions 22+) to a different valid character.
        // This keeps the string decodeable but changes the encryption key,
        // which must cause an AES-GCM authentication failure.
        let mut chars: Vec<char> = claim_id.chars().collect();
        chars[30] = if chars[30] == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();

        let result = vault.retrieve_proof(Zeroizing::new(tampered)).await;
        // Either DecryptionFailed (wrong key → auth tag mismatch) or
        // ProofNotFound (corrupted record_id lookup).
        assert!(result.is_err());
    }
}
