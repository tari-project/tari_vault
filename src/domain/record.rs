use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The raw cryptographic payload written to the storage backend.
/// Contains no plaintext data — safe to persist to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedRecord {
    /// AES-256-GCM nonce (12 bytes), stored as a base64 string.
    #[serde(with = "base64_field")]
    pub nonce: Vec<u8>,

    /// AES-256-GCM ciphertext + auth tag, stored as a base64 string.
    #[serde(with = "base64_field")]
    pub ciphertext: Vec<u8>,
}

/// Full record written to the storage file, including metadata for TTL support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRecord {
    #[serde(flatten)]
    pub encrypted: EncryptedRecord,

    /// UTC timestamp when the record was stored.
    pub stored_at: DateTime<Utc>,

    /// Optional UTC expiry time. `None` means the record never expires.
    pub expires_at: Option<DateTime<Utc>>,
}

impl StoredRecord {
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|t| Utc::now() > t).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn is_expired_none_means_never_expires() {
        let record = StoredRecord {
            encrypted: EncryptedRecord {
                nonce: vec![],
                ciphertext: vec![],
            },
            stored_at: Utc::now(),
            expires_at: None,
        };
        assert!(!record.is_expired());
    }

    #[test]
    fn is_expired_future_expiry_is_not_expired() {
        let record = StoredRecord {
            encrypted: EncryptedRecord {
                nonce: vec![],
                ciphertext: vec![],
            },
            stored_at: Utc::now(),
            expires_at: Some(Utc::now() + Duration::hours(1)),
        };
        assert!(!record.is_expired());
    }

    #[test]
    fn is_expired_past_expiry_is_expired() {
        let record = StoredRecord {
            encrypted: EncryptedRecord {
                nonce: vec![],
                ciphertext: vec![],
            },
            stored_at: Utc::now() - Duration::hours(2),
            expires_at: Some(Utc::now() - Duration::hours(1)),
        };
        assert!(record.is_expired());
    }
}

/// Serde helper: serialise `Vec<u8>` as a standard base64 string.
mod base64_field {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}
