use thiserror::Error;

pub type MgResult<T> = Result<T, MgError>;

#[derive(Debug, Error)]
pub enum MgError {
    #[error("invalid package name: {0}")]
    InvalidPackageName(String),
    #[error("invalid package spec: {0}")]
    InvalidPackageSpec(String),
    #[error("invalid version: {0}")]
    InvalidVersion(String),
    #[error("invalid version range: {0}")]
    InvalidVersionRange(String),
    #[error("dependency conflict: {0}")]
    DependencyConflict(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("store error: {0}")]
    Store(String),
    /// A capability that is not implemented for this core must fail closed
    /// with guidance — NEVER report success (Ok) for work not performed.
    /// Capability chưa implement cho core này phải fail-closed kèm hướng
    /// dẫn — TUYỆT ĐỐI không trả success (Ok) cho việc chưa làm.
    #[error("{core} core does not support '{capability}' yet: {guidance}")]
    Unsupported {
        /// Ecosystem/core that lacks the capability.
        core: &'static str,
        /// Capability name (resolve, fetch, install, audit, ...).
        capability: &'static str,
        /// Actionable remediation for the user.
        guidance: String,
    },
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
