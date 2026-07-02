//! Catalog system for shared version pinning.
//!
//! Catalogs define version specs in a single place (`mgpm.yaml`) and let
//! packages reference them via `catalog:` or `catalog:<name>` syntax.
//!
//! # Modes
//!
//! | Mode | Behavior |
//! |------|----------|
//! | `Manual` | Only resolve explicit `catalog:` references |
//! | `Prefer` | Auto-use catalog version if available; allow direct overrides |
//! | `Strict` | ALL dependencies must use `catalog:` references |
//!
//! # Examples
//!
//! ```yaml
//! catalog:
//!   react: "^19.0.0"
//!   typescript: "~5.7.0"
//! catalogs:
//!   react18:
//!     react: "^18.3.0"
//! ```

use std::collections::{HashMap, HashSet};

use mgpm_core::Catalog;
use thiserror::Error;

/// Catalog mode controls how strictly catalog references are enforced.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CatalogMode {
    /// Only resolve explicit `catalog:` specifiers.
    #[default]
    Manual,
    /// Prefer catalog version but allow direct version specs.
    Prefer,
    /// Require all dependencies to use a catalog reference.
    Strict,
}

/// Source of a resolved version from a catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogSource {
    /// Resolved from the default catalog.
    Default,
    /// Resolved from a named catalog.
    Named(String),
}

/// A resolved version specification with its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSpec {
    pub version: String,
    pub source: CatalogSource,
}

/// Errors that can occur during catalog resolution.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CatalogError {
    #[error("catalog '{0}' not found")]
    CatalogNotFound(String),

    #[error("package '{0}' not found in catalog{1}")]
    PackageNotInCatalog(String, String),

    #[error("catalog mode is strict, but dependency '{0}' does not use a catalog reference")]
    StrictViolation(String),

    #[error("catalog reference syntax error: '{0}'")]
    InvalidSyntax(String),

    #[error("{0}")]
    Internal(String),
}

/// Resolves dependency versions from workspace catalogs.
///
/// Catalogs provide centralized version management for monorepo dependencies.
/// See the [module-level documentation](self) for details.
#[derive(Debug, Clone)]
pub struct CatalogResolver {
    /// Default catalog (keyed as "default" in config).
    default: Catalog,
    /// Named catalogs.
    named: HashMap<String, Catalog>,
    /// Catalog mode.
    mode: CatalogMode,
}

impl CatalogResolver {
    /// Build a resolver from the workspace configuration.
    ///
    /// Reads `config.catalogs` — the entry keyed `"default"` becomes the
    /// default catalog; all other entries become named catalogs.
    pub fn from_config(config: &mgpm_core::MgpmConfig) -> Self {
        let mut default = Catalog::default();
        let mut named = HashMap::new();

        for (name, cat) in &config.catalogs {
            if name == "default" {
                default = cat.clone();
            } else {
                named.insert(name.clone(), cat.clone());
            }
        }

        Self {
            default,
            named,
            mode: CatalogMode::Manual,
        }
    }

    /// Create a resolver with explicit catalogs.
    pub fn new(default: Catalog, named: HashMap<String, Catalog>, mode: CatalogMode) -> Self {
        Self {
            default,
            named,
            mode,
        }
    }

    /// Set the catalog mode.
    pub fn with_mode(mut self, mode: CatalogMode) -> Self {
        self.mode = mode;
        self
    }

    /// Check whether a dependency specifier is a catalog reference.
    ///
    /// Returns `true` for `"catalog:"` and `"catalog:<name>"`.
    pub fn is_catalog_reference(specifier: &str) -> bool {
        specifier == "catalog:" || specifier.starts_with("catalog:")
    }

    /// Parse a catalog reference specifier.
    ///
    /// - `"catalog:"` → `Ok(None)` (default catalog)
    /// - `"catalog:react18"` → `Ok(Some("react18"))` (named catalog)
    /// - anything else → `Err(InvalidSyntax)`
    pub fn parse_catalog_ref(specifier: &str) -> Result<Option<String>, CatalogError> {
        if !specifier.starts_with("catalog:") {
            return Err(CatalogError::InvalidSyntax(specifier.to_string()));
        }
        let rest = &specifier[8..];
        if rest.is_empty() {
            Ok(None)
        } else {
            Ok(Some(rest.to_string()))
        }
    }

    /// Resolve a package version from a catalog.
    ///
    /// * `catalog_name = None` — look up in the default catalog.
    /// * `catalog_name = Some("react18")` — look up in that named catalog.
    pub fn resolve(
        &self,
        package: &str,
        catalog_name: Option<&str>,
    ) -> Result<VersionSpec, CatalogError> {
        match catalog_name {
            None => {
                let ver = self.default.packages.get(package).ok_or_else(|| {
                    CatalogError::PackageNotInCatalog(
                        package.to_string(),
                        " (default)".to_string(),
                    )
                })?;
                Ok(VersionSpec {
                    version: ver.clone(),
                    source: CatalogSource::Default,
                })
            }
            Some(name) => {
                let cat = self
                    .named
                    .get(name)
                    .ok_or_else(|| CatalogError::CatalogNotFound(name.to_string()))?;
                let ver = cat.packages.get(package).ok_or_else(|| {
                    CatalogError::PackageNotInCatalog(
                        package.to_string(),
                        format!(" ({name})"),
                    )
                })?;
                Ok(VersionSpec {
                    version: ver.clone(),
                    source: CatalogSource::Named(name.to_string()),
                })
            }
        }
    }

    /// Resolve a single dependency entry.
    ///
    /// If the specifier is a `catalog:` reference, resolves it.
    /// Otherwise returns the specifier unchanged (or in `Prefer` mode, tries
    /// the default catalog).
    pub fn resolve_dependency(
        &self,
        package: &str,
        specifier: &str,
    ) -> Result<String, CatalogError> {
        if Self::is_catalog_reference(specifier) {
            let catalog_name = Self::parse_catalog_ref(specifier)?;
            let resolved = self.resolve(package, catalog_name.as_deref())?;
            return Ok(resolved.version);
        }
        if self.mode == CatalogMode::Prefer {
            if let Some(ver) = self.default.packages.get(package) {
                return Ok(ver.clone());
            }
        }
        Ok(specifier.to_string())
    }

    /// Replace `catalog:` references in a dependency map with actual versions.
    ///
    /// In `Prefer` mode also rewrites direct versions that differ from the
    /// default catalog.
    pub fn rewrite_dependencies(
        &self,
        deps: &mut HashMap<String, String>,
    ) -> Result<(), CatalogError> {
        let mut errors = Vec::new();
        let keys: Vec<String> = deps.keys().cloned().collect();

        for pkg in keys {
            let specifier = deps[&pkg].clone();
            if Self::is_catalog_reference(&specifier) {
                match self.resolve_dependency(&pkg, &specifier) {
                    Ok(version) => {
                        deps.insert(pkg, version);
                    }
                    Err(e) => {
                        errors.push(e);
                    }
                }
            } else if self.mode == CatalogMode::Prefer {
                if let Some(ver) = self.default.packages.get(&pkg) {
                    if specifier != *ver {
                        deps.insert(pkg, ver.clone());
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.remove(0))
        }
    }

    /// Validate all dependencies against catalog rules.
    ///
    /// In `Strict` mode, every dependency must use a `catalog:` reference.
    /// Non-catalog references produce a `StrictViolation` error.
    ///
    /// Returns `Ok(())` if valid, or `Err(Vec<CatalogError>)` with all errors.
    pub fn validate(
        &self,
        deps: &HashMap<String, String>,
    ) -> Result<(), Vec<CatalogError>> {
        let mut errors = Vec::new();

        for (pkg, specifier) in deps {
            if Self::is_catalog_reference(specifier) {
                match Self::parse_catalog_ref(specifier) {
                    Ok(catalog_name) => {
                        if let Err(e) = self.resolve(pkg, catalog_name.as_deref()) {
                            errors.push(e);
                        }
                    }
                    Err(e) => errors.push(e),
                }
            } else if self.mode == CatalogMode::Strict {
                errors.push(CatalogError::StrictViolation(pkg.clone()));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Add a package version to the default catalog.
    pub fn add_to_default(&mut self, package: &str, version: &str) {
        self.default
            .packages
            .insert(package.to_string(), version.to_string());
    }

    /// Get a reference to the default catalog.
    pub fn default_catalog(&self) -> &Catalog {
        &self.default
    }

    /// Get a reference to a named catalog.
    pub fn named_catalog(&self, name: &str) -> Option<&Catalog> {
        self.named.get(name)
    }

    /// Collect all package names across all catalogs.
    pub fn all_packages(&self) -> HashSet<&str> {
        let mut pkgs = HashSet::new();
        for pkg in self.default.packages.keys() {
            pkgs.insert(pkg.as_str());
        }
        for cat in self.named.values() {
            for pkg in cat.packages.keys() {
                pkgs.insert(pkg.as_str());
            }
        }
        pkgs
    }

    /// Check whether a package exists in any catalog.
    pub fn has_package(&self, package: &str) -> bool {
        self.default.packages.contains_key(package)
            || self
                .named
                .values()
                .any(|c| c.packages.contains_key(package))
    }

    /// Total number of catalogs (including the default).
    pub fn catalog_count(&self) -> usize {
        1 + self.named.len()
    }

    /// Total number of catalog entries across all catalogs.
    pub fn entry_count(&self) -> usize {
        let mut count = self.default.packages.len();
        for cat in self.named.values() {
            count += cat.packages.len();
        }
        count
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn default_config() -> mgpm_core::MgpmConfig {
        let mut cfg = mgpm_core::MgpmConfig::default();
        cfg.catalogs.insert(
            "default".to_string(),
            Catalog {
                packages: HashMap::from([
                    ("react".to_string(), "^19.0.0".to_string()),
                    ("next".to_string(), "^15.0.0".to_string()),
                    ("typescript".to_string(), "~5.7.0".to_string()),
                ]),
            },
        );
        cfg.catalogs.insert(
            "react18".to_string(),
            Catalog {
                packages: HashMap::from([
                    ("react".to_string(), "^18.3.0".to_string()),
                    ("react-dom".to_string(), "^18.3.0".to_string()),
                ]),
            },
        );
        cfg
    }

    #[test]
    fn test_resolve_default() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let react = resolver.resolve("react", None).unwrap();
        assert_eq!(react.version, "^19.0.0");
        assert_eq!(react.source, CatalogSource::Default);

        let next = resolver.resolve("next", None).unwrap();
        assert_eq!(next.version, "^15.0.0");
    }

    #[test]
    fn test_resolve_named() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let react18 = resolver.resolve("react", Some("react18")).unwrap();
        assert_eq!(react18.version, "^18.3.0");
        assert_eq!(react18.source, CatalogSource::Named("react18".into()));
    }

    #[test]
    fn test_resolve_missing_package() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let err = resolver.resolve("nonexistent", None).unwrap_err();
        assert!(
            matches!(&err, CatalogError::PackageNotInCatalog(pkg, _) if pkg == "nonexistent")
        );
    }

    #[test]
    fn test_catalog_not_found() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let err = resolver.resolve("react", Some("nonexistent")).unwrap_err();
        assert!(matches!(&err, CatalogError::CatalogNotFound(name) if name == "nonexistent"));
    }

    #[test]
    fn test_parse_catalog_ref() {
        assert_eq!(
            CatalogResolver::parse_catalog_ref("catalog:").unwrap(),
            None
        );
        assert_eq!(
            CatalogResolver::parse_catalog_ref("catalog:react18")
                .unwrap(),
            Some("react18".to_string())
        );
        assert!(
            CatalogResolver::parse_catalog_ref("npm:react").is_err()
        );
        assert!(CatalogResolver::parse_catalog_ref("^19.0.0").is_err());
    }

    #[test]
    fn test_is_catalog_reference() {
        assert!(CatalogResolver::is_catalog_reference("catalog:"));
        assert!(CatalogResolver::is_catalog_reference("catalog:react18"));
        assert!(!CatalogResolver::is_catalog_reference("^19.0.0"));
        assert!(!CatalogResolver::is_catalog_reference("npm:react"));
    }

    #[test]
    fn test_rewrite_dependencies() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let mut deps = HashMap::from([
            ("react".to_string(), "catalog:".to_string()),
            ("next".to_string(), "catalog:".to_string()),
            ("lodash".to_string(), "^4.17.0".to_string()),
        ]);

        resolver.rewrite_dependencies(&mut deps).unwrap();
        assert_eq!(deps.get("react").unwrap(), "^19.0.0");
        assert_eq!(deps.get("next").unwrap(), "^15.0.0");
        assert_eq!(deps.get("lodash").unwrap(), "^4.17.0");
    }

    #[test]
    fn test_rewrite_catalog_ref_error() {
        let config = mgpm_core::MgpmConfig::default();
        let resolver = CatalogResolver::from_config(&config);

        let mut deps = HashMap::from([
            ("react".to_string(), "catalog:".to_string()),
        ]);

        let err = resolver.rewrite_dependencies(&mut deps).unwrap_err();
        assert!(matches!(err, CatalogError::PackageNotInCatalog(_, _)));
    }

    #[test]
    fn test_validate_strict_passes() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config).with_mode(CatalogMode::Strict);

        let deps = HashMap::from([
            ("react".to_string(), "catalog:".to_string()),
            ("next".to_string(), "catalog:".to_string()),
        ]);

        assert!(resolver.validate(&deps).is_ok());
    }

    #[test]
    fn test_validate_strict_violation() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config).with_mode(CatalogMode::Strict);

        let deps = HashMap::from([
            ("react".to_string(), "catalog:".to_string()),
            ("lodash".to_string(), "^4.17.0".to_string()),
        ]);

        let errs = resolver.validate(&deps).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], CatalogError::StrictViolation(pkg) if pkg == "lodash"));
    }

    #[test]
    fn test_validate_manual_ignores_direct_versions() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config).with_mode(CatalogMode::Manual);

        let deps = HashMap::from([
            ("react".to_string(), "catalog:".to_string()),
            ("lodash".to_string(), "^4.17.0".to_string()),
        ]);

        assert!(resolver.validate(&deps).is_ok());
    }

    #[test]
    fn test_validate_bad_catalog_ref() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let deps = HashMap::from([
            ("react".to_string(), "catalog:nonexistent".to_string()),
        ]);

        let errs = resolver.validate(&deps).unwrap_err();
        assert!(!errs.is_empty());
    }

    #[test]
    fn test_add_to_default() {
        let mut resolver = CatalogResolver::new(
            Catalog::default(),
            HashMap::new(),
            CatalogMode::Manual,
        );

        resolver.add_to_default("react", "^19.0.0");
        assert!(resolver.has_package("react"));
        assert_eq!(
            resolver.resolve("react", None).unwrap().version,
            "^19.0.0"
        );
    }

    #[test]
    fn test_all_packages() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let pkgs = resolver.all_packages();
        assert!(pkgs.contains("react"));
        assert!(pkgs.contains("next"));
        assert!(pkgs.contains("typescript"));
        assert!(pkgs.contains("react-dom"));
        assert_eq!(pkgs.len(), 4);
    }

    #[test]
    fn test_has_package() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        assert!(resolver.has_package("react"));
        assert!(resolver.has_package("react-dom"));
        assert!(!resolver.has_package("nonexistent"));
    }

    #[test]
    fn test_count() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        assert_eq!(resolver.catalog_count(), 2);
        assert_eq!(resolver.entry_count(), 5);
    }

    #[test]
    fn test_prefer_mode_rewrites() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config).with_mode(CatalogMode::Prefer);

        let mut deps = HashMap::from([
            ("react".to_string(), "^18.0.0".to_string()),
            ("next".to_string(), "^15.0.0".to_string()),
            ("lodash".to_string(), "^4.17.0".to_string()),
        ]);

        resolver.rewrite_dependencies(&mut deps).unwrap();
        // react differs from catalog → rewritten
        assert_eq!(deps.get("react").unwrap(), "^19.0.0");
        // next matches catalog → unchanged
        assert_eq!(deps.get("next").unwrap(), "^15.0.0");
        // lodash not in catalog → unchanged
        assert_eq!(deps.get("lodash").unwrap(), "^4.17.0");
    }

    #[test]
    fn test_resolve_dependency_direct() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let result = resolver
            .resolve_dependency("lodash", "^4.17.0")
            .unwrap();
        assert_eq!(result, "^4.17.0");
    }

    #[test]
    fn test_resolve_dependency_catalog() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let result = resolver.resolve_dependency("react", "catalog:").unwrap();
        assert_eq!(result, "^19.0.0");

        let result = resolver
            .resolve_dependency("react", "catalog:react18")
            .unwrap();
        assert_eq!(result, "^18.3.0");
    }

    #[test]
    fn test_resolve_dependency_prefer() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config).with_mode(CatalogMode::Prefer);

        // In Prefer mode, a direct version that exists in catalog gets rewritten
        let result = resolver
            .resolve_dependency("react", "^18.0.0")
            .unwrap();
        assert_eq!(result, "^19.0.0");
    }

    #[test]
    fn test_default_catalog_accessor() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let default = resolver.default_catalog();
        assert!(default.packages.contains_key("react"));
    }

    #[test]
    fn test_named_catalog_accessor() {
        let config = default_config();
        let resolver = CatalogResolver::from_config(&config);

        let cat = resolver.named_catalog("react18").unwrap();
        assert!(cat.packages.contains_key("react-dom"));
        assert!(resolver.named_catalog("nonexistent").is_none());
    }
}
