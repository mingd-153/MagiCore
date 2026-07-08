//! Error types for mg
//!
//! All mg errors are defined here, with structured error handling
//! and error reporting support.

use thiserror::Error;

/// Result type alias for mg operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for mg
#[derive(Debug, Error)]
pub enum Error {
    // ============ Core Errors ============
    /// Generic error with a message
    #[error("{0}")]
    Generic(String),
    
    // ============ Package Errors ============
    /// Package name is invalid
    #[error("invalid package name: {0}")]
    InvalidPackageName(#[from] crate::PackageNameError),
    
    /// Package ID is invalid
    #[error("invalid package ID: {0}")]
    InvalidPackageId(#[from] crate::PackageIdError),
    
    /// Dependency specification is invalid
    #[error("invalid dependency spec: {0}")]
    InvalidDependencySpec(#[from] crate::DependencySpecError),
    
    /// Semver error
    #[error("semver error: {0}")]
    Semver(#[from] crate::SemVerError),
    
    // ============ Resolver Errors ============
    /// No matching version found
    #[error("no matching version for {package}@{requirement}")]
    NoMatchingVersion { package: String, requirement: String },
    
    /// Dependency conflict detected
    #[error("dependency conflict: {0}")]
    DependencyConflict(String),
    
    /// Circular dependency detected
    #[error("circular dependency: {path}")]
    CircularDependency { path: String },
    
    /// Workspace package not found
    #[error("workspace package not found: {name}")]
    WorkspacePackageNotFound { name: String },
    
    // ============ Store/CAFS Errors ============
    /// File not found in store
    #[error("file not found in store: {hash}")]
    StoreFileNotFound { hash: String },
    
    /// Store integrity check failed
    #[error("store integrity check failed for {package}@{version}")]
    StoreIntegrityMismatch { package: String, version: String },
    
    /// Store write failed
    #[error("store write error: {0}")]
    StoreWrite(String),
    
    /// Store read failed
    #[error("store read error: {0}")]
    StoreRead(String),
    
    /// Store corruption detected
    #[error("store corruption: {0}")]
    StoreCorruption(String),
    
    // ============ Network/Registry Errors ============
    /// Network request failed
    #[error("network error: {0}")]
    NetworkError(#[from] NetworkError),
    
    /// Registry error
    #[error("registry error ({registry}): {message}")]
    RegistryError { registry: String, message: String },
    
    /// Package not found in registry
    #[error("package not found: {package}@{version} in {registry}")]
    PackageNotFound { package: String, version: String, registry: String },
    
    /// Rate limit exceeded
    #[error("rate limit exceeded for {registry} (retry after {retry_after}s)")]
    RateLimitExceeded { registry: String, retry_after: u64 },
    
    /// Authentication required
    #[error("authentication required for {registry}")]
    AuthenticationRequired { registry: String },
    
    /// Authentication failed
    #[error("authentication failed for {registry}")]
    AuthenticationFailed { registry: String },
    
    // ============ Lockfile Errors ============
    /// Lockfile not found
    #[error("lockfile not found")]
    LockfileNotFound,
    
    /// Lockfile parse error
    #[error("lockfile parse error: {0}")]
    LockfileParse(String),
    
    /// Lockfile version mismatch
    #[error("lockfile version mismatch: found {found}, expected {expected}")]
    LockfileVersionMismatch { found: u32, expected: u32 },
    
    /// Lockfile outdated
    #[error("lockfile is outdated - run 'mg install' to update")]
    LockfileOutdated,
    
    /// Lockfile write failed
    #[error("lockfile write error: {0}")]
    LockfileWrite(String),
    
    // ============ Linker Errors ============
    /// Link operation failed
    #[error("link error: {0}")]
    LinkError(String),
    
    /// Symlink creation failed
    #[error("symlink creation failed: {path}")]
    SymlinkFailed { path: String },
    
    /// Hardlink creation failed (cross-device)
    #[error("hardlink failed (cross-device): {path}")]
    HardlinkCrossDevice { path: String },
    
    /// node_modules structure invalid
    #[error("invalid node_modules: {0}")]
    InvalidNodeModules(String),
    
    // ============ Config Errors ============
    /// Config error
    #[error("config error: {0}")]
    Config(#[from] crate::config::ConfigError),
    
    /// Config not found
    #[error("config file not found: {path}")]
    ConfigNotFound { path: String },
    
    // ============ Installer Errors ============
    /// Download failed
    #[error("download failed: {0}")]
    DownloadFailed(String),
    
    /// Tarball extraction failed
    #[error("tarball extract error: {0}")]
    TarballExtract(String),
    
    /// Integrity verification failed
    #[error("integrity check failed for {package}@{version}")]
    IntegrityCheckFailed { package: String, version: String },
    
    /// Installation failed
    #[error("install error: {0}")]
    InstallFailed(String),
    
    // ============ Plugin Errors ============
    /// Plugin error
    #[error("plugin error: {0}")]
    Plugin(String),
    
    /// Plugin not found
    #[error("plugin not found: {name}")]
    PluginNotFound { name: String },
    
    /// Plugin hook failed
    #[error("plugin hook '{hook}' failed in {plugin}")]
    PluginHookFailed { hook: String, plugin: String },
    
    // ============ Workspace Errors ============
    /// Workspace error
    #[error("workspace error: {0}")]
    Workspace(String),
    
    /// No workspace found
    #[error("no workspace found in {path}")]
    NoWorkspace { path: String },
    
    // ============ System Errors ============
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Path error
    #[error("invalid path: {0}")]
    InvalidPath(String),
    
    /// Permission denied
    #[error("permission denied: {path}")]
    PermissionDenied { path: String },
    
    /// Disk full
    #[error("disk full: {path}")]
    DiskFull { path: String },
}

impl Error {
    /// Returns true if this error indicates a user-facing problem.
    pub fn is_user_facing(&self) -> bool {
        matches!(
            self,
            Error::Generic(_)
                | Error::InvalidPackageName(_)
                | Error::InvalidPackageId(_)
                | Error::InvalidDependencySpec(_)
                | Error::NoMatchingVersion { .. }
                | Error::DependencyConflict(_)
                | Error::CircularDependency { .. }
                | Error::WorkspacePackageNotFound { .. }
                | Error::PackageNotFound { .. }
                | Error::RateLimitExceeded { .. }
                | Error::AuthenticationRequired { .. }
                | Error::LockfileNotFound
                | Error::LockfileOutdated
                | Error::ConfigNotFound { .. }
                | Error::PluginNotFound { .. }
                | Error::NoWorkspace { .. }
                | Error::PermissionDenied { .. }
                | Error::DiskFull { .. }
        )
    }
    
    /// Returns a short error code for programmatic handling.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Generic(_) => "EUNKNOWN",
            Error::InvalidPackageName(_) => "EINVALID_NAME",
            Error::InvalidPackageId(_) => "EINVALID_ID",
            Error::InvalidDependencySpec(_) => "EINVALID_DEP",
            Error::Semver(_) => "ESEMVER",
            Error::NoMatchingVersion { .. } => "ENOMATCH",
            Error::DependencyConflict(_) => "ECONFLICT",
            Error::CircularDependency { .. } => "ECIRCULAR",
            Error::WorkspacePackageNotFound { .. } => "EWORKSPACE_PKG",
            Error::StoreFileNotFound { .. } => "ENOTFOUND",
            Error::StoreIntegrityMismatch { .. } => "EINTEGRITY",
            Error::StoreWrite(_) => "ESTORE_WRITE",
            Error::StoreRead(_) => "ESTORE_READ",
            Error::StoreCorruption(_) => "ECORRUPTION",
            Error::NetworkError(_) => "ENETWORK",
            Error::RegistryError { .. } => "EREGISTRY",
            Error::PackageNotFound { .. } => "EPKG_NOT_FOUND",
            Error::RateLimitExceeded { .. } => "ERATE_LIMIT",
            Error::AuthenticationRequired { .. } => "EAUTH_REQUIRED",
            Error::AuthenticationFailed { .. } => "EAUTH_FAILED",
            Error::LockfileNotFound => "ELOCKFILE_NOT_FOUND",
            Error::LockfileParse(_) => "ELOCKFILE_PARSE",
            Error::LockfileVersionMismatch { .. } => "ELOCKFILE_VERSION",
            Error::LockfileOutdated => "ELOCKFILE_OUTDATED",
            Error::LockfileWrite(_) => "ELOCKFILE_WRITE",
            Error::LinkError(_) => "ELINK",
            Error::SymlinkFailed { .. } => "ESYMLINK",
            Error::HardlinkCrossDevice { .. } => "EHARDLINK_XDEV",
            Error::InvalidNodeModules(_) => "ENODE_MODULES",
            Error::Config(_) => "ECONFIG",
            Error::ConfigNotFound { .. } => "ECONFIG_NOT_FOUND",
            Error::DownloadFailed(_) => "EDOWNLOAD",
            Error::TarballExtract(_) => "EEXTRACT",
            Error::IntegrityCheckFailed { .. } => "EINTEGRITY_CHK",
            Error::InstallFailed(_) => "EINSTALL",
            Error::Plugin(_) => "EPLUGIN",
            Error::PluginNotFound { .. } => "EPLUGIN_NOT_FOUND",
            Error::PluginHookFailed { .. } => "EPLUGIN_HOOK",
            Error::Workspace(_) => "EWORKSPACE",
            Error::NoWorkspace { .. } => "ENOWORKSPACE",
            Error::Io(_) => "EIO",
            Error::InvalidPath(_) => "EINVALID_PATH",
            Error::PermissionDenied { .. } => "EPERMISSION",
            Error::DiskFull { .. } => "EDISKFULL",
        }
    }
}

/// Network error details
#[derive(Debug, Clone, Error)]
pub enum NetworkError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("timeout after {0}s")]
    Timeout(u64),
    
    #[error("connection reset")]
    ConnectionReset,
    
    #[error("name resolution failed: {0}")]
    DnsFailed(String),
    
    #[error("TLS error: {0}")]
    TlsError(String),
    
    #[error("proxy error: {0}")]
    ProxyError(String),
    
    #[error("request cancelled")]
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let err = Error::LockfileNotFound;
        assert_eq!(err.code(), "ELOCKFILE_NOT_FOUND");
        assert!(err.is_user_facing());
    }
}