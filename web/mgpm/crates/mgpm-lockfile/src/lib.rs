//! MGPM Lockfile Crate
//!
//! Dual format lockfile support: binary (lockb) and text (lock)

pub mod binary;
pub mod lockfile;
pub mod pipeline;
pub mod text;

pub use lockfile::{
    Lockfile, LockfileMetadata, LockfilePackage, PackageResolution, LOCKFILE_BINARY_EXT,
    LOCKFILE_MAGIC, LOCKFILE_TEXT_EXT, LOCKFILE_VERSION,
};
pub use pipeline::{PipelineError, ResolutionConfig, ResolutionPipeline, WantedDependency};

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

    #[error("content hash mismatch: expected {expected}, got {actual}")]
    ContentHashMismatch { expected: String, actual: String },

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
    use tempfile::tempdir;

    #[test]
    fn test_lockfile_error_from_io() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let lock_err = LockfileError::from(err);
        assert!(matches!(lock_err, LockfileError::Io(_)));
    }

    #[test]
    fn test_lockfile_roundtrip() {
        let temp = tempdir().unwrap();

        let mut original = Lockfile::new(1, "https://registry.npmjs.org");

        let pkg1 = LockfilePackage {
            id: "react@18.2.0".to_string(),
            name: "react".to_string(),
            version: "18.2.0".to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
                registry: Some("npm".to_string()),
            },
            integrity: Some("sha512-abc123".to_string()),
            dependencies: vec![],
        };

        let pkg2 = LockfilePackage {
            id: "lodash@4.17.21".to_string(),
            name: "lodash".to_string(),
            version: "4.17.21".to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz".to_string(),
                registry: Some("npm".to_string()),
            },
            integrity: Some("sha512-def456".to_string()),
            dependencies: vec![],
        };

        original.add_package(pkg1);
        original.add_package(pkg2);
        original.sort_packages();
        original.compute_content_hash();
        original.update_timestamp();

        // Text round-trip
        let text_path = temp.path().join("mgpm.lock");
        text::write_text(&original, &text_path).unwrap();
        let from_text = text::read_text(&text_path).unwrap();

        assert_eq!(from_text.packages.len(), original.packages.len());
        for (a, b) in from_text.packages.iter().zip(original.packages.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.version, b.version);
            assert_eq!(a.integrity, b.integrity);
            assert_eq!(a.resolution.r#type, b.resolution.r#type);
            assert_eq!(a.resolution.url, b.resolution.url);
        }

        // Binary round-trip
        let binary_path = temp.path().join("mgpm.lockb");
        binary::write_binary(&original, &binary_path).unwrap();
        let from_binary = binary::read_binary(&binary_path).unwrap();

        assert_eq!(from_binary.packages.len(), original.packages.len());
        for (a, b) in from_binary.packages.iter().zip(original.packages.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.version, b.version);
            assert_eq!(a.integrity, b.integrity);
            assert_eq!(a.resolution.r#type, b.resolution.r#type);
            assert_eq!(a.resolution.url, b.resolution.url);
        }

        // Cross-format consistency
        assert_eq!(from_text.packages.len(), from_binary.packages.len());
        for (t, b) in from_text.packages.iter().zip(from_binary.packages.iter()) {
            assert_eq!(t.name, b.name);
            assert_eq!(t.version, b.version);
        }
    }
}
