//! `mgc-lockfile` — Cryptographically signed lockfile for Zero-Trust Supply Chain
//! Lockfile ký mật mã cho chuỗi cung ứng Zero-Trust
//!
//! Provides lockfile schema v3 (canonical unified dependency graph) with
//! Ed25519 signatures for tamper detection. v2 files remain readable.
//! Cung cấp schema lockfile v3 (đồ thị phụ thuộc hợp nhất canonical) với
//! chữ ký Ed25519 chống tamper. File v2 vẫn đọc được.

pub mod atomic;
pub mod canonical;
pub mod ecosystem_tag;
pub mod export;
pub mod import;
pub mod merge;
pub mod migrate;
pub mod parser;
pub mod policy;
pub mod project_lock;
pub mod root_pin;
pub mod schema;
pub mod serialization;
pub mod v4;
pub mod v4_graph;
pub mod verifier;
pub mod writer;

pub use ecosystem_tag::EcosystemTag;
pub use export::{ExportTarget, export_canonical};
pub use import::{
    LegacyLockfile, check_trust_downgrade_risk, detect_legacy_lockfiles, import_file,
    import_into_lockfile,
};
pub use merge::{MergeConflict, merge3, resolve_git_conflict_markers};
pub use root_pin::{RootPin, format_root_pin, parse_root_pin, update_owner_root_pins};

pub use migrate::{
    auto_upgrade_lockfile, detect_lockfile_version, migrate_v1_to_v2, migrate_v2_to_v3,
    migrate_v3_to_v4,
};
pub use parser::{
    LockDocument, load_and_verify_lockfile, load_lock_document, load_lockfile, parse_document,
    parse_lockfile, read_bounded_regular_file, read_lockfile_bytes, read_signature_file_bytes,
    signature_file_presence,
};
pub use schema::{
    ArtifactRef, CrossEdge, OWNER_DELEGATED, OWNER_MGC_NATIVE, OWNER_SCAFFOLD_ONLY,
    OWNER_UNSUPPORTED, Provenance, SOURCE_KIND_DELEGATED_TOOL, SOURCE_KIND_NATIVE_RESOLVE,
    SOURCE_KIND_REGISTRY_IMPORT,
};
pub use schema::{
    LOCKFILE_SCHEMA_VERSION, Lockfile, LockfileMetadata, OwnershipEntry, Package, SignatureFile,
    SignerInfo, WorkspaceTopology, load_ownership_ledger,
};
pub use v4::{
    Edge, EdgeKind, EdgeOrigin, LOCKFILE_SCHEMA_V4, PackageKey, SignatureBlock, SourceRef,
    SourceSelectionError, TargetTuple, VariantKey, canonical_name, canonical_version,
    claim_matches, claim_specificity, peer_context_digest, select_source,
};
pub use verifier::{
    VerificationStatus, verification_status_message, verify_lockfile, verify_lockfile_with_trust,
    verify_ownership_completeness,
};
pub use writer::{
    ensure_lockfile_mutation_allowed, serialize_lockfile, sign_and_write_lockfile,
    sign_lockfile_with_default_key, write_lockfile,
};

// Issue #4: Lockfile V2 - Temporary stub, replace with proper v2 implementation
pub fn read_lockfile_checked(project_root: &std::path::Path) -> LockfileResult<Option<Lockfile>> {
    let lockfile_path = project_root.join("mgc.lock");
    match std::fs::symlink_metadata(&lockfile_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(LockfileError::ParseError(format!(
                "lockfile '{}' must be a regular non-symlink file",
                lockfile_path.display()
            )))
        }
        Ok(_) => load_lockfile(&lockfile_path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Legacy checksum-sidecar API. Checksums are not written by this crate;
/// lock integrity uses the signed-lockfile APIs instead.
/// API sidecar checksum cũ. Crate này không ghi checksum; integrity lock
/// được xử lý bằng API lockfile có chữ ký.
pub fn write_lockfile_checksum(
    _project_root: &std::path::Path,
    _content: &[u8],
) -> LockfileResult<()> {
    Err(LockfileError::WriteFailed(
        "lockfile checksum sidecars are unsupported; use the signed lockfile API".to_string(),
    ))
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

    /// A normal dependency mutation cannot rewrite a signed lock without
    /// an explicit re-sign transaction. The current v3 sidecar format is a
    /// two-file protocol, so silently replacing only `mgc.lock` would turn
    /// a successful install into a tampered lock.
    #[error("refusing to mutate signed mgc.lock: {0}")]
    SignedLockMutation(String),

    #[error("Lockfile tampered: {0}")]
    TamperedLockfile(String),

    #[error("Invalid signature file: {0}")]
    InvalidSignatureFile(String),

    /// Package entry lacks required provenance/registry data (v3 ownership
    /// completeness gate) — Entry package thiếu dữ liệu provenance/registry
    /// (cổng kiểm tra tính đầy đủ provenance của v3).
    #[error("Incomplete provenance: {0}")]
    IncompleteProvenance(String),

    /// Lock writer could not acquire the project lock in time — the lock
    /// is held by a live process; never steal it, surface LockBusy.
    /// (Không acquire được lock project đúng hạn — lock đang giữ bởi
    /// process sống; không bao giờ cướp, báo LockBusy.)
    #[error("project lock busy: {0}")]
    LockBusy(String),

    /// Lockfile signed by a key outside the trust roots — Khóa ký ngoài
    /// trust roots.
    #[error("untrusted signing key: {0}")]
    UntrustedKey(String),

    /// Lock write I/O failure (temp create/write/fsync/replace/dir-fsync)
    /// — Lỗi I/O ghi lock.
    #[error("lock write failed: {0}")]
    WriteFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParseError(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerializeError(#[from] toml::ser::Error),

    #[error("Crypto error: {0}")]
    CryptoError(#[from] mgc_crypto::CryptoError),

    /// A well-formed v4 document that cannot drive an install without
    /// projecting away identity (duplicate instances, missing SRI/URL,
    /// dangling edges) — callers must fail closed, never flatten.
    /// (Tài liệu v4 đúng shape nhưng không thể lái install mà không ép
    /// identity — caller phải fail-closed, không flatten.)
    #[error("v4 lockfile cannot drive install losslessly: {0}")]
    UnusableForInstall(String),
}
