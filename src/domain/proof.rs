use std::fmt;

use serde_json::Value;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::VaultError;

/// Memory-safe container for a plaintext L1 Merkle Proof.
///
/// # Security invariants
/// - Zeroes its contents on drop via `ZeroizeOnDrop`.
/// - Does NOT implement `Clone`, `Debug` (only redacted variant), or `Display`.
/// - Never appears in log output.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PlaintextProof {
    pub(crate) data: Vec<u8>,
}

impl PlaintextProof {
    /// Create from raw bytes (e.g. serialised JSON).
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Create by serialising any JSON value to bytes.
    pub fn from_json(value: &Value) -> Result<Self, VaultError> {
        let bytes =
            serde_json::to_vec(value).map_err(|e| VaultError::Serialization(e.to_string()))?;
        Ok(Self { data: bytes })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Deserialise the inner bytes as a JSON `Value`, then zeroize the buffer.
    pub fn into_json(mut self) -> Result<Value, VaultError> {
        let result = serde_json::from_slice(&self.data)
            .map_err(|e| VaultError::Serialization(e.to_string()));
        self.data.zeroize();
        result
    }
}

/// Redacted — never prints proof content.
impl fmt::Debug for PlaintextProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlaintextProof(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_json_and_into_json_round_trip() {
        let value = json!({"root": "abc", "path": [1, 2, 3]});
        let proof = PlaintextProof::from_json(&value).unwrap();
        let result = proof.into_json().unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn into_json_returns_error_on_invalid_bytes() {
        let proof = PlaintextProof::from_bytes(b"\xff\xfe invalid utf8".to_vec());
        assert!(proof.into_json().is_err());
    }

    #[test]
    fn debug_output_is_redacted() {
        let proof = PlaintextProof::from_bytes(b"super secret".to_vec());
        let debug = format!("{proof:?}");
        assert_eq!(debug, "PlaintextProof(<redacted>)");
        assert!(!debug.contains("super secret"));
    }
}
