//! `mgc-lockfile` — Cryptographically signed lockfile for Zero-Trust Supply Chain
//! Lockfile ký mật mã cho chuỗi cung ứng Zero-Trust
//!
//! Provides lockfile schema v2 with Ed25519 signatures for tamper detection.

pub mod import;
pub mod merge;
pub mod migrate;
pub mod parser;
pub mod schema;
pub mod serialization;
pub mod verifier;
pub mod writer;

pub use import::{
    check_trust_downgrade_risk, detect_legacy_lockfiles, import_file, import_into_lockfile,
    LegacyLockfile,
};
pub use merge::{merge3, resolve_git_conflict_markers, MergeConflict};

pub use migrate::{auto_upgrade_lockfile, detect_lockfile_version, migrate_v1_to_v2};
pub use parser::{load_and_verify_lockfile, load_lockfile, parse_lockfile};
pub use schema::{Lockfile, LockfileMetadata, Package, SignatureFile, SignerInfo};
pub use verifier::{verification_status_message, verify_lockfile, VerificationStatus};
pub use writer::{
    serialize_lockfile, sign_and_write_lockfile, sign_lockfile_with_default_key, write_lockfile,
};

// FIXME(V1.0.1): Temporary stub — replace with proper v2 implementation
pub fn read_lockfile_checked(project_root: &std::path::Path) -> LockfileResult<Option<Lockfile>> {
    let lockfile_path = project_root.join("mgc.lock");
    if !lockfile_path.exists() {
        return Ok(None);
    }
    load_lockfile(&lockfile_path).map(Some)
}

// FIXME(V1.0.1): Temporary stub — checksum writing deferred to v2
pub fn write_lockfile_checksum(
    _project_root: &std::path::Path,
    _content: &[u8],
) -> LockfileResult<()> {
    // Checksum validation moved to signature verification in v2
    Ok(())
}

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
    CryptoError(#[from] mgc_crypto::CryptoError),
}
