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
