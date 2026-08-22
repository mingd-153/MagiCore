//! `mg-crypto` — Cryptographic primitives for MegaGate supply chain security
//! Cryptographic primitives cho bảo mật chuỗi cung ứng MegaGate
//!
//! Provides BLAKE3 hashing, Ed25519 signing/verification, keyring management,
//! and SIMD-accelerated operations.

pub mod blake3_signer;
pub mod ed25519_signer;
pub mod integrity;
pub mod keyring;
pub mod simd;

pub use blake3_signer::{Blake3Hasher, Blake3Hash};
pub use ed25519_signer::{Ed25519Signer, Ed25519Signature, Ed25519PublicKey};
pub use integrity::{IntegrityVerifier, SriHash};
pub use keyring::{Keyring, KeyPair};
pub use simd::{SimdCapability, detect_simd};

/// Result type for crypto operations — Kiểu kết quả cho thao tác crypto
pub type CryptoResult<T> = Result<T, CryptoError>;

/// Errors for crypto operations — Lỗi cho thao tác crypto
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Blake3 hash failed: {0}")]
    Blake3Failed(String),

    #[error("Ed25519 signing failed: {0}")]
    SigningFailed(String),

    #[error("Ed25519 verification failed: {0}")]
    VerificationFailed(String),

    #[error("Keyring operation failed: {0}")]
    KeyringFailed(String),

    #[error("Invalid signature format: {0}")]
    InvalidSignature(String),

    #[error("Invalid public key format: {0}")]
    InvalidPublicKey(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Base64 decode error: {0}")]
    Base64Error(#[from] base64::DecodeError),
}
