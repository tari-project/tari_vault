//! # tari_vault
//!
//! A secure, encrypted intermediary layer for handing off L1 Merkle Proofs
//! to an L2 wallet daemon without exposing plaintext proof material to
//! intermediaries (users, AI agents, orchestration code).
//!
//! ## Security model
//!
//! The **Key-in-the-ID** pattern is used:
//! - The `Claim_ID` returned by `store_proof` encodes both a storage lookup key
//!   and the AES-256-GCM encryption key in a single base64url string.
//! - The vault server only persists the ciphertext and nonce — the encryption
//!   key *never* reaches disk.
//! - All sensitive structs implement [`zeroize::ZeroizeOnDrop`] so key material
//!   is wiped from RAM as soon as it leaves scope.
//! - Claims are single-use: the record is deleted on the first successful
//!   retrieval.
//!
//! ## Quick start (embedding the library)
//!
//! ```rust,no_run
//! use tari_vault::{
//!     storage::LocalFileStore,
//!     vault::{ProofVault, StandardVault},
//!     domain::PlaintextProof,
//! };
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let store = LocalFileStore::new(PathBuf::from("/tmp/vault.json"))?;
//! let vault = StandardVault::new(store);
//!
//! // Sender side
//! let proof = PlaintextProof::from_bytes(b"{\"root\":\"abc\"}".to_vec());
//! let claim_id = vault.store_proof(proof, Some(3600)).await?;
//!
//! // Receiver side (claim_id passed via untrusted channel)
//! let proof = vault.retrieve_proof(claim_id).await?;
//! let json  = proof.into_json()?;
//! # Ok(()) }
//! ```

pub mod auth;
pub mod config;
pub mod domain;
pub mod error;
pub mod rpc;
pub mod storage;
pub mod vault;
