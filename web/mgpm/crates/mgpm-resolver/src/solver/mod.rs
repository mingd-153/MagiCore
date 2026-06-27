//! PubGrub-based dependency resolver

pub mod pubgrub;

use std::collections::{HashMap, HashSet};
use std::fmt;

use mgpm_core::{PackageId, PackageName, Version};

use super::version::VersionSet;

pub struct Resolver {
    provider: Box<dyn DependencyProvider>,
    catalogs: HashMap<String, HashMap<String, String>>,
    overrides: HashMap<String, String>,
}

pub trait DependencyProvider: Send + Sync {
    fn get_versions(&self, package: &PackageName) -> Vec<Version>;
    fn get_dependencies(&self, package_id: &PackageId) -> Vec<ResolvedDep>;
}

#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub package: PackageName,
    pub spec: String,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub package_id: PackageId,
    pub version: Version,
    pub integrity: String,
    pub deps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub resolutions: Vec<Resolution>,
}

impl Resolver {
    pub fn new(provider: Box<dyn DependencyProvider>) -> Self {
        Self {
            provider,
            catalogs: HashMap::new(),
            overrides: HashMap::new(),
        }
    }

    pub fn solve(&self, wanted: &[(PackageName, String)]) -> Result<SolveResult, SolveError> {
        let mut resolutions = Vec::new();
        let mut seen = HashSet::new();
        
        for (name, _spec) in wanted {
            if seen.contains(name) {
                continue;
            }
            seen.insert(name.clone());
            
            let versions = self.provider.get_versions(name);
            if let Some(version) = versions.last() {
                let package_id = PackageId::new(name.clone(), version.clone());
                resolutions.push(Resolution {
                    package_id: package_id.clone(),
                    version: version.clone(),
                    integrity: format!("sha256-{}", hex::encode(name.as_str())),
                    deps: Vec::new(),
                });
                
                for dep in self.provider.get_dependencies(&package_id) {
                    if !seen.contains(&dep.package) {
                        seen.insert(dep.package.clone());
                    }
                }
            }
        }

        Ok(SolveResult { resolutions })
    }
}

#[derive(Debug, Clone)]
pub struct SolveError {
    pub message: String,
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SolveError {}

impl From<String> for SolveError {
    fn from(s: String) -> Self {
        Self { message: s }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    impl DependencyProvider for MockProvider {
        fn get_versions(&self, package: &PackageName) -> Vec<Version> {
            vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ]
        }

        fn get_dependencies(&self, _package_id: &PackageId) -> Vec<ResolvedDep> {
            vec![]
        }
    }

    #[test]
    fn test_solve_simple() {
        let resolver = Resolver::new(Box::new(MockProvider));
        let wanted = vec![
            (PackageName::new("react").unwrap(), "^1.0.0".to_string()),
        ];
        
        let result = resolver.solve(&wanted).unwrap();
        assert_eq!(result.resolutions.len(), 1);
    }
}