use std::collections::HashSet;

use mg_core::PackageName;

use crate::lockfile::{Lockfile, LOCKFILE_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum LockfileVerifyError {
    #[error("content hash mismatch: lockfile has been modified")]
    ContentHashMismatch,

    #[error("not all packages are resolved in lockfile")]
    NotAllResolved,

    #[error("package count mismatch: expected {expected}, got {actual}")]
    PackageCountMismatch { expected: usize, actual: usize },

    #[error("package '{0}' not found in lockfile")]
    MissingPackage(String),

    #[error("version mismatch for '{package}': lockfile has {lockfile}, expected {expected}")]
    VersionMismatch {
        package: String,
        lockfile: String,
        expected: String,
    },

    #[error("unsupported lockfile version: {0}")]
    UnsupportedVersion(u32),
}

impl Lockfile {
    pub fn is_fresh(&self) -> bool {
        if self.version != LOCKFILE_VERSION {
            return false;
        }
        let mut hasher = blake3::Hasher::new();

        let mut sorted = self.packages.clone();
        sorted.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));

        for pkg in &sorted {
            hasher.update(pkg.name.as_bytes());
            hasher.update(b"\0");
            hasher.update(pkg.version.as_bytes());
            hasher.update(b"\0");
            if let Some(ref integrity) = pkg.integrity {
                hasher.update(integrity.as_bytes());
            }
            hasher.update(b"\0");
            hasher.update(pkg.resolution.r#type.as_bytes());
            hasher.update(b"\0");
            hasher.update(pkg.resolution.url.as_bytes());
            hasher.update(b"\0");
            if let Some(ref registry) = pkg.resolution.registry {
                hasher.update(registry.as_bytes());
            }
            hasher.update(b"\0");
            hasher.update(&[pkg.resolved as u8]);
            hasher.update(b"\0");
        }

        hasher.update(self.metadata.registry.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.metadata.config_version.to_string().as_bytes());

        let computed = hasher.finalize().to_hex().to_string();
        computed == self.metadata.content_hash
    }

    pub fn verify_freshness(
        &self,
        wanted: &[(PackageName, String)],
    ) -> Result<(), LockfileVerifyError> {
        if self.version != LOCKFILE_VERSION {
            return Err(LockfileVerifyError::UnsupportedVersion(self.version));
        }

        if !self.is_fresh() {
            return Err(LockfileVerifyError::ContentHashMismatch);
        }

        let all_resolved = self.packages.iter().all(|p| p.resolved);
        if !all_resolved {
            return Err(LockfileVerifyError::NotAllResolved);
        }

        if self.packages.len() != wanted.len() {
            return Err(LockfileVerifyError::PackageCountMismatch {
                expected: wanted.len(),
                actual: self.packages.len(),
            });
        }

        let mut seen = HashSet::new();
        for (name, _spec) in wanted {
            if seen.contains(name) {
                continue;
            }
            seen.insert(name.clone());

            if !self.packages.iter().any(|p| p.name == name.as_str()) {
                return Err(LockfileVerifyError::MissingPackage(name.to_string()));
            }
        }

        Ok(())
    }

    pub fn get_resolved_version(&self, name: &str) -> Option<&str> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.resolved)
            .map(|p| p.version.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{Lockfile, LockfileMetadata, LockfilePackage, PackageResolution, LOCKFILE_VERSION};

    fn make_test_lockfile() -> Lockfile {
        let mut lockfile = Lockfile {
            version: LOCKFILE_VERSION,
            metadata: LockfileMetadata {
                config_version: 1,
                created_at: 0,
                updated_at: 0,
                content_hash: String::new(),
                registry: "https://registry.npmjs.org".into(),
            },
            packages: vec![LockfilePackage {
                id: "lodash@4.17.21".into(),
                name: "lodash".into(),
                version: "4.17.21".into(),
                resolution: PackageResolution {
                    r#type: "registry".into(),
                    url: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz".into(),
                    registry: Some("npm".into()),
                },
                integrity: Some("sha256-abc".into()),
                dependencies: vec![],
                resolved: true,
                resolved_at: Some(12345),
            }],
        };
        lockfile.compute_content_hash();
        lockfile
    }

    #[test]
    fn test_is_fresh_valid() {
        let lockfile = make_test_lockfile();
        assert!(lockfile.is_fresh());
    }

    #[test]
    fn test_is_fresh_tampered() {
        let mut lockfile = make_test_lockfile();
        lockfile.packages[0].version = "5.0.0".into();
        assert!(!lockfile.is_fresh());
    }

    #[test]
    fn test_verify_freshness_ok() {
        let lockfile = make_test_lockfile();
        let wanted = vec![(PackageName::new("lodash").unwrap(), "^4.0.0".into())];
        assert!(lockfile.verify_freshness(&wanted).is_ok());
    }

    #[test]
    fn test_verify_freshness_missing_package() {
        let lockfile = make_test_lockfile();
        let wanted = vec![
            (PackageName::new("lodash").unwrap(), "^4.0.0".into()),
            (PackageName::new("react").unwrap(), "^18.0.0".into()),
        ];
        assert!(lockfile.verify_freshness(&wanted).is_err());
    }

    #[test]
    fn test_get_resolved_version() {
        let lockfile = make_test_lockfile();
        assert_eq!(
            lockfile.get_resolved_version("lodash"),
            Some("4.17.21")
        );
        assert!(lockfile.get_resolved_version("nonexistent").is_none());
    }
}
