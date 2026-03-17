use std::fmt;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::error::VaultError;

/// Number of bytes in the combined ClaimId binary payload.
///
/// Layout: `record_id[16] || encryption_key[32]` = 48 bytes → 64-char base64url string.
const RECORD_ID_LEN: usize = 16;
const ENCRYPTION_KEY_LEN: usize = 32;
const CLAIM_ID_BYTES: usize = RECORD_ID_LEN + ENCRYPTION_KEY_LEN;

/// In-memory representation of a Claim ID.
///
/// # Security invariants
/// - `encryption_key` is zeroized on drop.
/// - The struct does NOT implement `Clone` or `Display` so the key cannot be
///   accidentally copied or printed.
/// - Debug output redacts the key.
#[derive(ZeroizeOnDrop)]
pub struct ClaimId {
    /// Non-sensitive storage lookup key (UUIDv4 bytes).
    pub record_id: [u8; RECORD_ID_LEN],
    /// AES-256 encryption key — MUST NEVER be written to persistent storage.
    pub encryption_key: [u8; ENCRYPTION_KEY_LEN],
}

impl ClaimId {
    /// Generate a new ClaimId for the given encryption key.
    pub fn new(encryption_key: [u8; ENCRYPTION_KEY_LEN]) -> Self {
        Self {
            record_id: *Uuid::new_v4().as_bytes(),
            encryption_key,
        }
    }

    /// Encode as a base64url (no-padding) string.
    ///
    /// The combined bytes are wrapped in `Zeroizing` so they are wiped
    /// from the stack immediately after encoding.
    pub fn encode(&self) -> String {
        let mut bytes = Zeroizing::new([0u8; CLAIM_ID_BYTES]);
        bytes[..RECORD_ID_LEN].copy_from_slice(&self.record_id);
        bytes[RECORD_ID_LEN..].copy_from_slice(&self.encryption_key);
        URL_SAFE_NO_PAD.encode(bytes.as_ref())
    }

    /// Decode from a base64url string.
    ///
    /// Decoded bytes are wrapped in `Zeroizing` so the raw key material is
    /// wiped from the heap as soon as it is split into the struct fields.
    pub fn decode(s: &str) -> Result<Self, VaultError> {
        let bytes = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(s)
                .map_err(|_| VaultError::InvalidClaimId)?,
        );

        if bytes.len() != CLAIM_ID_BYTES {
            return Err(VaultError::InvalidClaimId);
        }

        let mut record_id = [0u8; RECORD_ID_LEN];
        let mut encryption_key = [0u8; ENCRYPTION_KEY_LEN];
        record_id.copy_from_slice(&bytes[..RECORD_ID_LEN]);
        encryption_key.copy_from_slice(&bytes[RECORD_ID_LEN..]);

        Ok(Self {
            record_id,
            encryption_key,
        })
    }

    /// Format `record_id` as a lowercase hex string — safe for log output.
    pub fn record_id_hex(&self) -> String {
        Uuid::from_bytes(self.record_id).simple().to_string()
    }
}

/// Only the non-sensitive `record_id` is included — the key is redacted.
impl fmt::Debug for ClaimId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaimId")
            .field("record_id", &self.record_id_hex())
            .field("encryption_key", &"<redacted>")
            .finish()
    }
}

impl Zeroize for ClaimId {
    fn zeroize(&mut self) {
        self.record_id.zeroize();
        self.encryption_key.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_encode_decode() {
        let key = [0x42u8; 32];
        let claim = ClaimId::new(key);
        let encoded = claim.encode();

        // Encoded string should be exactly 64 characters (48 raw bytes in base64url no-pad).
        assert_eq!(encoded.len(), 64);

        let decoded = ClaimId::decode(&encoded).unwrap();
        assert_eq!(decoded.record_id, claim.record_id);
        assert_eq!(decoded.encryption_key, claim.encryption_key);
    }

    #[test]
    fn decode_rejects_wrong_length() {
        assert!(ClaimId::decode("tooshort").is_err());
        assert!(ClaimId::decode(&"A".repeat(70)).is_err());
    }

    #[test]
    fn decode_rejects_invalid_base64() {
        assert!(
            ClaimId::decode("not!valid@base64#string$here%and^more&here*so(we)have-enough+chars")
                .is_err()
        );
    }
}
