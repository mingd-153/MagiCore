//! Ed25519 signing and verification using ring
//! Ed25519 ký và verify sử dụng ring

use crate::{CryptoError, CryptoResult};
use ring::signature::{Ed25519KeyPair, KeyPair as _, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};

/// Ed25519 signature (64 bytes) — Ed25519 chữ ký (64 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ed25519Signature(pub Vec<u8>);

impl Ed25519Signature {
    /// Convert to base64 string — Chuyển sang chuỗi base64
    pub fn to_base64(&self) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.0)
    }

    /// Parse from base64 string — Parse từ chuỗi base64
    pub fn from_base64(s: &str) -> CryptoResult<Self> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)?;
        if bytes.len() != 64 {
            return Err(CryptoError::InvalidSignature(format!(
                "expected 64 bytes, got {}",
                bytes.len()
            )));
        }
        Ok(Ed25519Signature(bytes))
    }
}

/// Ed25519 public key (32 bytes) — Ed25519 khóa công khai (32 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ed25519PublicKey(pub Vec<u8>);

impl Ed25519PublicKey {
    /// Convert to base64 string — Chuyển sang chuỗi base64
    pub fn to_base64(&self) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.0)
    }

    /// Parse from base64 string — Parse từ chuỗi base64
    pub fn from_base64(s: &str) -> CryptoResult<Self> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)?;
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidPublicKey(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        Ok(Ed25519PublicKey(bytes))
    }
}

/// Ed25519 signer — Ed25519 ký
pub struct Ed25519Signer {
    key_pair: Ed25519KeyPair,
}

impl Ed25519Signer {
    /// Generate new key pair from seed — Tạo key pair mới từ seed
    pub fn from_seed(seed: &[u8; 32]) -> CryptoResult<Self> {
        let key_pair = Ed25519KeyPair::from_seed_unchecked(seed)
            .map_err(|e| CryptoError::SigningFailed(format!("invalid seed: {:?}", e)))?;
        Ok(Self { key_pair })
    }

    /// Generate new key pair from PKCS8 bytes — Tạo key pair từ PKCS8 bytes
    pub fn from_pkcs8(pkcs8_bytes: &[u8]) -> CryptoResult<Self> {
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes)
            .map_err(|e| CryptoError::SigningFailed(format!("invalid pkcs8: {:?}", e)))?;
        Ok(Self { key_pair })
    }

    /// Get public key — Lấy khóa công khai
    pub fn public_key(&self) -> Ed25519PublicKey {
        Ed25519PublicKey(self.key_pair.public_key().as_ref().to_vec())
    }

    /// Sign message — Ký message
    pub fn sign(&self, message: &[u8]) -> Ed25519Signature {
        let signature = self.key_pair.sign(message);
        Ed25519Signature(signature.as_ref().to_vec())
    }

    /// Sign string — Ký chuỗi
    pub fn sign_string(&self, s: &str) -> Ed25519Signature {
        self.sign(s.as_bytes())
    }
}

/// Verify Ed25519 signature — Verify chữ ký Ed25519
pub fn verify_signature(
    public_key: &Ed25519PublicKey,
    message: &[u8],
    signature: &Ed25519Signature,
) -> CryptoResult<()> {
    let public_key_unparsed = UnparsedPublicKey::new(&ED25519, &public_key.0);
    public_key_unparsed
        .verify(message, &signature.0)
        .map_err(|e| CryptoError::VerificationFailed(format!("signature invalid: {:?}", e)))
}

/// Verify signature for string — Verify chữ ký cho chuỗi
pub fn verify_string_signature(
    public_key: &Ed25519PublicKey,
    s: &str,
    signature: &Ed25519Signature,
) -> CryptoResult<()> {
    verify_signature(public_key, s.as_bytes(), signature)
}
