//! MGPM Lockfile Crate
//!
//! Dual format lockfile support: binary (lockb) and text (lock)

pub mod binary;
pub mod lockfile;
pub mod pipeline;
pub mod text;

pub use lockfile::{
    Lockfile, LockfileMetadata, LockfilePackage, PackageResolution,
    LOCKFILE_BINARY_EXT, LOCKFILE_TEXT_EXT, LOCKFILE_MAGIC, LOCKFILE_VERSION,
};
pub use pipeline::{ResolutionPipeline, ResolutionConfig, WantedDependency, PipelineError};

#[derive(Debug, thiserror::Error)]
pub enum LockfileError {
    #[error("IO error: {0}")]
    Io(String),
    
    #[error("serialization error: {0}")]
    Serialization(String),
    
    #[error("deserialization error: {0}")]
    Deserialization(String),
    
    #[error("invalid magic number")]
    InvalidMagic,
    
    #[error("version mismatch: found {found}, expected {expected}")]
    VersionMismatch { found: u32, expected: u32 },
    
    #[error("lockfile not found: {0}")]
    NotFound(String),
    
    #[error("lockfile corrupted: {0}")]
    Corrupted(String),
    
    #[error("lockfile outdated")]
    Outdated,
}

impl From<std::io::Error> for LockfileError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<toml::ser::Error> for LockfileError {
    fn from(e: toml::ser::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<toml::de::Error> for LockfileError {
    fn from(e: toml::de::Error) -> Self {
        Self::Deserialization(e.to_string())
    }
}

impl From<bincode::Error> for LockfileError {
    fn from(e: bincode::Error) -> Self {
        Self::Deserialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_error_from_io() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let lock_err = LockfileError::from(err);
        assert!(matches!(lock_err, LockfileError::Io(_)));
    }
}
