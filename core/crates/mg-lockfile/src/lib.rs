//! `mg-lockfile` — Cryptographically signed lockfile for Zero-Trust Supply Chain
//! Lockfile ký mật mã cho chuỗi cung ứng Zero-Trust
//!
//! Provides lockfile schema v2 with Ed25519 signatures for tamper detection.

pub mod schema;
pub mod parser;
pub mod writer;
pub mod verifier;
pub mod migrate;

pub use schema::{Lockfile, LockfileMetadata, Package, SignatureFile, SignerInfo};
pub use parser::{load_and_verify_lockfile, load_lockfile, parse_lockfile};
pub use writer::{sign_and_write_lockfile, sign_lockfile_with_default_key, write_lockfile};
pub use verifier::{verify_lockfile, verification_status_message, VerificationStatus};
pub use migrate::{auto_upgrade_lockfile, detect_lockfile_version, migrate_v1_to_v2};

/// Result type for lockfile operations — Kiểu kết quả cho thao tác lockfile
pub type LockfileResult<T> = Result<T, LockfileError>;

/// Errors for lockfile operations — Lỗi cho thao tác lockfile
#[derive(Debug, thiserror::Error)]
pub enum LockfileError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Signature verification failed: {0}")]
    VerificationFailed(String),

    #[error("Lockfile tampered: {0}")]
    TamperedLockfile(String),

    #[error("Invalid signature file: {0}")]
    InvalidSignatureFile(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParseError(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerializeError(#[from] toml::ser::Error),

    #[error("Crypto error: {0}")]
    CryptoError(#[from] mg_crypto::CryptoError),
}
