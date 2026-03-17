use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Proof not found")]
    ProofNotFound,

    #[error("Proof has expired")]
    ProofExpired,

    /// Returned for ALL cryptographic failures — generic message prevents oracle attacks.
    #[error("Decryption failed")]
    DecryptionFailed,

    #[error("Invalid claim ID format")]
    InvalidClaimId,

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl VaultError {
    /// JSON-RPC error code for this variant.
    pub fn rpc_code(&self) -> i32 {
        match self {
            Self::ProofNotFound => -32001,
            Self::ProofExpired => -32002,
            Self::InvalidClaimId => -32003,
            Self::DecryptionFailed => -32004,
            Self::Storage(_) | Self::Serialization(_) => -32005,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_error_rpc_codes() {
        assert_eq!(VaultError::ProofNotFound.rpc_code(), -32001);
        assert_eq!(VaultError::ProofExpired.rpc_code(), -32002);
        assert_eq!(VaultError::InvalidClaimId.rpc_code(), -32003);
        assert_eq!(VaultError::DecryptionFailed.rpc_code(), -32004);
        assert_eq!(
            VaultError::Storage(StorageError::NotFound).rpc_code(),
            -32005
        );
        assert_eq!(VaultError::Serialization("oops".into()).rpc_code(), -32005);
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Record not found")]
    NotFound,
}
