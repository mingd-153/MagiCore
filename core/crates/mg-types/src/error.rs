//! Unified error types for MegaGate

use thiserror::Error;

/// Result type alias for MegaGate operations
pub type MgResult<T> = Result<T, MgError>;

/// Top-level error type for all MegaGate operations
#[derive(Debug, Error)]
pub enum MgError {
    // Package errors
    #[error("package not found: {0}")]
    PackageNotFound(String),

    #[error("invalid package name: {0}")]
    InvalidPackageName(String),

    #[error("version not found: {package}@{version}")]
    VersionNotFound { package: String, version: String },

    #[error("invalid version: {0}")]
    InvalidVersion(String),

    // Resolution errors
    #[error("dependency conflict: {0}")]
    DependencyConflict(String),

    #[error("circular dependency detected: {0}")]
    CircularDependency(String),

    // Network errors
    #[error("network error: {0}")]
    Network(String),

    #[error("registry error ({status}): {message}")]
    Registry { status: u16, message: String },

    #[error("timeout fetching {url}")]
    Timeout { url: String },

    // Storage errors
    #[error("store error: {0}")]
    Store(String),

    #[error("integrity check failed for {package}: expected {expected}, got {actual}")]
    IntegrityMismatch {
        package: String,
        expected: String,
        actual: String,
    },

    // Security errors
    #[error("typosquatting detected: '{name}' is suspicious (similar to '{similar}')")]
    TyposquatDetected { name: String, similar: String },

    #[error("package too new: '{package}' was published {age_hours}h ago (minimum: {min_hours}h)")]
    PackageTooNew {
        package: String,
        age_hours: u64,
        min_hours: u64,
    },

    #[error("CVE vulnerability: {package}@{version} has {severity} severity: {cve}")]
    SecurityVulnerability {
        package: String,
        version: String,
        severity: String,
        cve: String,
    },

    // Lockfile errors
    #[error("lockfile parse error: {0}")]
    LockfileParse(String),

    #[error("lockfile integrity failed: expected hash {expected}")]
    LockfileIntegrity { expected: String },

    // Adapter errors
    #[error("adapter '{name}' not found for project type")]
    AdapterNotFound { name: String },

    #[error("adapter error ({adapter}): {message}")]
    Adapter { adapter: String, message: String },

    // Template/Scaffold errors
    #[error("template not found: {0}")]
    TemplateNotFound(String),

    #[error("template error: {0}")]
    Template(String),

    // IO errors
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    // Generic
    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl From<crate::package::PackageNameError> for MgError {
    fn from(e: crate::package::PackageNameError) -> Self {
        MgError::InvalidPackageName(e.to_string())
    }
}

impl From<crate::package::VersionRangeError> for MgError {
    fn from(e: crate::package::VersionRangeError) -> Self {
        MgError::Other(format!("invalid version range: {e}"))
    }
}

impl From<serde_json::Error> for MgError {
    fn from(e: serde_json::Error) -> Self {
        MgError::Other(format!("JSON error: {e}"))
    }
}

impl MgError {
    /// Create a package not found error
    pub fn not_found(package: impl Into<String>) -> Self {
        Self::PackageNotFound(package.into())
    }

    /// Create a generic other error
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// Create a network error
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    /// Create a store error
    pub fn store(msg: impl Into<String>) -> Self {
        Self::Store(msg.into())
    }

    /// Returns true if this is a network error (retryable)
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::Timeout { .. })
    }

    /// Returns true if this is a security error
    pub fn is_security(&self) -> bool {
        matches!(
            self,
            Self::TyposquatDetected { .. }
                | Self::PackageTooNew { .. }
                | Self::SecurityVulnerability { .. }
                | Self::IntegrityMismatch { .. }
        )
    }
}
