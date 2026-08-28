//! SBOM generator from lockfile
//! Tạo SBOM từ lockfile

use crate::cyclonedx::*;
use crate::{SbomOptions, SbomResult};
use mgc_lockfile::Lockfile;
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

