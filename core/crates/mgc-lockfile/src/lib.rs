//! `mgc-lockfile` — Cryptographically signed lockfile for Zero-Trust Supply Chain
//! Lockfile ký mật mã cho chuỗi cung ứng Zero-Trust
//!
//! Provides lockfile schema v3 (canonical unified dependency graph) with
//! Ed25519 signatures for tamper detection. v2 files remain readable.
//! Cung cấp schema lockfile v3 (đồ thị phụ thuộc hợp nhất canonical) với
//! chữ ký Ed25519 chống tamper. File v2 vẫn đọc được.

pub mod ecosystem_tag;
pub mod export;
pub mod import;
pub mod merge;
pub mod migrate;
pub mod parser;
pub mod schema;
pub mod serialization;
pub mod verifier;
pub mod writer;

pub use ecosystem_tag::EcosystemTag;
pub use export::{ExportTarget, export_canonical};
pub use import::{
    LegacyLockfile, check_trust_downgrade_risk, detect_legacy_lockfiles, import_file,
    import_into_lockfile,
};
pub use merge::{MergeConflict, merge3, resolve_git_conflict_markers};

pub use migrate::{
    auto_upgrade_lockfile, detect_lockfile_version, migrate_v1_to_v2, migrate_v2_to_v3,
};
pub use parser::{load_and_verify_lockfile, load_lockfile, parse_lockfile};
pub use schema::{
    ArtifactRef, CrossEdge, OWNER_DELEGATED, OWNER_MGC_NATIVE, OWNER_SCAFFOLD_ONLY,
    OWNER_UNSUPPORTED, Provenance, SOURCE_KIND_DELEGATED_TOOL, SOURCE_KIND_NATIVE_RESOLVE,
    SOURCE_KIND_REGISTRY_IMPORT,
};
pub use schema::{
    LOCKFILE_SCHEMA_VERSION, Lockfile, LockfileMetadata, OwnershipEntry, Package, SignatureFile,
    SignerInfo, WorkspaceTopology, load_ownership_ledger,
};
pub use verifier::{
    VerificationStatus, verification_status_message, verify_lockfile, verify_ownership_completeness,
};
pub use writer::{
    serialize_lockfile, sign_and_write_lockfile, sign_lockfile_with_default_key, write_lockfile,
};

// Issue #4: Lockfile V2 - Temporary stub, replace with proper v2 implementation
pub fn read_lockfile_checked(project_root: &std::path::Path) -> LockfileResult<Option<Lockfile>> {
    let lockfile_path = project_root.join("mgc.lock");
    if !lockfile_path.exists() {
        return Ok(None);
    }
    load_lockfile(&lockfile_path).map(Some)
}

// Issue #4: Lockfile V2 - Temporary stub, checksum writing deferred to v2
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

    /// Package entry lacks required provenance/registry data (v3 ownership
    /// completeness gate) — Entry package thiếu dữ liệu provenance/registry
    /// (cổng kiểm tra tính đầy đủ provenance của v3).
    #[error("Incomplete provenance: {0}")]
    IncompleteProvenance(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParseError(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerializeError(#[from] toml::ser::Error),

    #[error("Crypto error: {0}")]
    CryptoError(#[from] mgc_crypto::CryptoError),
}
