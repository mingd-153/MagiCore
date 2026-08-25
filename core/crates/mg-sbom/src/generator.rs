//! SBOM generator from lockfile
//! Tạo SBOM từ lockfile

use crate::cyclonedx::*;
use crate::{SbomOptions, SbomResult};
use mg_lockfile::Lockfile;
use std::collections::HashMap;

/// SBOM generator — Trình tạo SBOM
pub struct SbomGenerator {
    options: SbomOptions,
}

impl SbomGenerator {
    /// Create new generator — Tạo generator mới
    pub fn new(options: SbomOptions) -> Self {
        Self { options }
    }

    /// Generate SBOM from lockfile — Tạo SBOM từ lockfile
    pub fn generate(&self, lockfile: &Lockfile) -> SbomResult<Bom> {
        let mut bom = Bom::new();

        // Convert packages to components
        let mut components = Vec::new();
        let mut dependency_map: HashMap<String, Vec<String>> = HashMap::new();

        for pkg in &lockfile.packages {
            let bom_ref = format!("pkg:{}@{}", pkg.name, pkg.version);

            // Create component
            let mut component = Component {
                component_type: ComponentType::Library,
                bom_ref: bom_ref.clone(),
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                purl: Some(format!("pkg:npm/{}@{}", pkg.name, pkg.version)),
                hashes: None,
                licenses: None,
            };

            // Add hashes if requested
            if self.options.include_hashes && !pkg.integrity.is_empty() {
                // Parse integrity (e.g., "blake3:abc123...")
                if let Some((alg, content)) = pkg.integrity.split_once(':') {
                    component.hashes = Some(vec![Hash {
                        alg: alg.to_uppercase(),
                        content: content.to_string(),
                    }]);
                }
            }

            components.push(component);

            // Build dependency graph
            if !pkg.dependencies.is_empty() {
                let deps: Vec<String> = pkg
                    .dependencies
                    .iter()
                    .map(|dep| {
                        // Parse "name@version" format
                        format!("pkg:{}", dep)
                    })
                    .collect();
                dependency_map.insert(bom_ref, deps);
            }
        }

        bom.components = components;

        // Add dependencies graph
        let dependencies: Vec<Dependency> = dependency_map
            .into_iter()
            .map(|(dep_ref, depends_on)| Dependency {
                dependency_ref: dep_ref,
                depends_on: Some(depends_on),
            })
            .collect();

        if !dependencies.is_empty() {
            bom.dependencies = Some(dependencies);
        }

        Ok(bom)
    }

    /// Generate SBOM JSON string — Tạo chuỗi JSON SBOM
    pub fn generate_json(&self, lockfile: &Lockfile) -> SbomResult<String> {
        let bom = self.generate(lockfile)?;
        let json = serde_json::to_string_pretty(&bom)?;
        Ok(json)
    }
}

impl Default for SbomGenerator {
    fn default() -> Self {
        Self::new(SbomOptions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_lockfile::{LockfileMetadata, Package};

    #[test]
    fn test_generate_sbom() {
        let lockfile = Lockfile {
            version: "2".to_string(),
            metadata: LockfileMetadata {
                generated_at: "2026-08-21T00:00:00Z".to_string(),
                generator: "mg/0.4.0".to_string(),
                lockfile_hash: "abc123".to_string(),
                signer: None,
            },
            packages: vec![
                Package {
                    name: "react".to_string(),
                    version: "18.2.0".to_string(),
                    resolved: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
                    integrity: "blake3:abc123".to_string(),
                    dependencies: vec![],
                },
                Package {
                    name: "lodash".to_string(),
                    version: "4.17.21".to_string(),
                    resolved: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"
                        .to_string(),
                    integrity: "blake3:def456".to_string(),
                    dependencies: vec![],
                },
            ],
        };

        let generator = SbomGenerator::default();
        let bom = generator.generate(&lockfile).unwrap();

        assert_eq!(bom.bom_format, "CycloneDX");
        assert_eq!(bom.spec_version, "1.5");
        assert_eq!(bom.components.len(), 2);

        // Check first component
        let react = &bom.components[0];
        assert_eq!(react.name, "react");
        assert_eq!(react.version, "18.2.0");
        assert_eq!(react.purl, Some("pkg:npm/react@18.2.0".to_string()));

        // Check hashes
        assert!(react.hashes.is_some());
        let hashes = react.hashes.as_ref().unwrap();
        assert_eq!(hashes[0].alg, "BLAKE3");
        assert_eq!(hashes[0].content, "abc123");
    }

    #[test]
    fn test_generate_json() {
        let lockfile = Lockfile {
            version: "2".to_string(),
            metadata: LockfileMetadata {
                generated_at: "2026-08-21T00:00:00Z".to_string(),
                generator: "mg/0.4.0".to_string(),
                lockfile_hash: "abc123".to_string(),
                signer: None,
            },
            packages: vec![Package {
                name: "test-pkg".to_string(),
                version: "1.0.0".to_string(),
                resolved: "https://example.com/test.tgz".to_string(),
                integrity: "blake3:test".to_string(),
                dependencies: vec![],
            }],
        };

        let generator = SbomGenerator::default();
        let json = generator.generate_json(&lockfile).unwrap();

        assert!(json.contains("CycloneDX"));
        assert!(json.contains("test-pkg"));
        assert!(json.contains("1.0.0"));
    }
}
