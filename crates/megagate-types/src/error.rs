use thiserror::Error;
#[cfg(feature = "uniffi")]
use uniffi::Error as UniffiError;

#[derive(Error, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "uniffi", derive(UniffiError))]
pub enum MegagateError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Version conflict: {0}")]
    VersionConflict(String),

    #[error("Integrity check failed: expected {expected}, got {actual}")]
    IntegrityMismatch { expected: String, actual: String },

    #[error("Registry error: {0}")]
    RegistryError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Lockfile error: {0}")]
    LockfileError(String),

    #[error("Workspace error: {0}")]
    WorkspaceError(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, MegagateError>;