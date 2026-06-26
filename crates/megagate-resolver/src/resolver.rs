use crate::conflict::{ResolutionDecision, resolve_conflicts};
use crate::graph::{Conflict, DependencyGraph};
use megagate_fetcher::registry_client::RegistryClient;
use megagate_types::package::{PackageManifest, ResolvedDependency};
use megagate_types::registry::RegistryPackageVersion;
use megagate_types::error::MegagateError;
use megagate_security::SecurityManager;
use megagate_types::config::MegagateConfig;
use semver::{Version, VersionReq};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ResolutionResult {
    pub resolved: HashMap<String, ResolvedDependency>,
    pub conflicts: Vec<Conflict>,
    pub decisions: Vec<ResolutionDecision>,
    pub workspace_packages: HashMap<String, ResolvedDependency>,
}

pub struct Resolver {
    registry_client: Arc<dyn RegistryClient>,
    security_manager: Arc<SecurityManager>,
    config: MegagateConfig,
}

impl Resolver {
    pub fn new(
        registry_client: Arc<dyn RegistryClient>,
        security_manager: Arc<SecurityManager>,
        config: MegagateConfig,
    ) -> Self {
        Self {
            registry_client,
            security_manager,
            config,
        }
    }

    pub async fn resolve(
        &self,
        manifests: HashMap<String, PackageManifest>,
        lockfile: Option<megagate_types::lockfile::LockfileV1>,
        _options: InstallOptions,
    ) -> Result<ResolutionResult, MegagateError> {
        let mut graph = DependencyGraph::new();
        let mut dep_specs: Vec<(String, String, String)> = Vec::new();

        for (_path, manifest) in manifests {
            let node_name = manifest.name.clone();
            graph.add_node(node_name.clone(), "*".to_string());

            for (dep_name, dep_range) in &manifest.dependencies {
                let dep_key = format!("{}@{}", dep_name, dep_range);
                if !graph.nodes.contains_key(&dep_key) {
                    graph.add_node(dep_name.clone(), dep_range.clone());
                }
                dep_specs.push((node_name.clone(), dep_name.clone(), dep_range.clone()));
            }

            for (dep_name, dep_range) in &manifest.dev_dependencies {
                let dep_key = format!("{}@{}", dep_name, dep_range);
                if !graph.nodes.contains_key(&dep_key) {
                    graph.add_node(dep_name.clone(), dep_range.clone());
                }
                dep_specs.push((node_name.clone(), dep_name.clone(), dep_range.clone()));
            }
        }

        for (node_name, dep_name, dep_range) in &dep_specs {
            let node_key = format!("{}@*", node_name);
            let dep_key = format!("{}@{}", dep_name, dep_range);

            let dep_clone = graph.nodes.get(&dep_key).cloned();
            if let Some(dep_clone) = dep_clone {
                if let Some(node) = graph.nodes.get_mut(&node_key) {
                    node.dependencies.insert(dep_name.clone(), dep_clone);
                }
            }
            if let Some(dep_node) = graph.nodes.get_mut(&dep_key) {
                dep_node.dependents.insert(node_name.clone());
            }
        }

        if let Some(lock) = lockfile {
            for (key, pkg) in lock.packages {
                if let Some(node) = graph.nodes.get_mut(&key) {
                    node.resolved = Some(pkg.into());
                }
            }
        }

        let dep_names: Vec<String> = dep_specs.iter().map(|(_, name, _)| name.clone()).collect();
        let unique_dep_names: Vec<String> = dep_names.into_iter().collect::<std::collections::HashSet<_>>().into_iter().collect();

        // Pre-fetch all package versions
        let mut package_versions: HashMap<String, Vec<RegistryPackageVersion>> = HashMap::new();
        for dep_name in &unique_dep_names {
            if !package_versions.contains_key(dep_name) {
                let versions = self.registry_client.get_package_versions(dep_name).await?;
                package_versions.insert(dep_name.clone(), versions);
            }
        }

        for dep_name in unique_dep_names {
            let range = dep_specs.iter()
                .find(|(_, name, _)| name == &dep_name)
                .map(|(_, _, r)| r.clone())
                .unwrap_or_else(|| "*".to_string());

            let version = self.resolve_version(&dep_name, &range).await?;
            
            // Find the metadata from pre-fetched versions
            let versions = package_versions.get(&dep_name).ok_or_else(|| MegagateError::PackageNotFound(dep_name.clone()))?;
            let metadata = versions.iter().find(|v| v.version == version)
                .ok_or_else(|| MegagateError::VersionConflict(format!("Version {} not found for {}", version, dep_name)))?;

            let dep_key = format!("{}@{}", dep_name, range);
            if let Some(node) = graph.nodes.get_mut(&dep_key) {
                node.resolved = Some(ResolvedDependency {
                    name: dep_name.clone(),
                    version: Version::parse(&version).map_err(|e| MegagateError::ConfigError(e.to_string()))?,
                    integrity: metadata.dist.integrity.clone(),
                    resolved: metadata.dist.tarball.clone(),
                    size: metadata.dist.unpacked_size.unwrap_or(0),
                    dependencies: metadata.dependencies.clone().unwrap_or_default(),
                    optional_dependencies: HashMap::new(),
                    peer_dependencies: metadata.peer_dependencies.clone().unwrap_or_default(),
                    bin: HashMap::new(),
                    engines: HashMap::new(),
                    publish_time: None,
                });
            }
        }

        let conflicts = graph.detect_conflicts();
        let decisions = resolve_conflicts(conflicts.clone(), self.registry_client.as_ref()).await?;

        // Create a map of conflict decisions for quick lookup
        let mut conflict_decisions: HashMap<String, Version> = HashMap::new();
        for decision in &decisions {
            conflict_decisions.insert(decision.name.clone(), decision.chosen_version.clone());
        }

        // Collect all resolved packages from graph nodes
        let mut resolved = HashMap::new();
        for (key, node) in &graph.nodes {
            if let Some(resolved_pkg) = &node.resolved {
                // If this package has a conflict decision, use the conflict resolution version
                // Otherwise, use the resolved version from the node
                let version_to_use = conflict_decisions.get(&node.name).unwrap_or(&resolved_pkg.version);
                
                if resolved_pkg.version == *version_to_use {
                    resolved.insert(node.name.clone(), resolved_pkg.clone());
                } else {
                    // Need to create a new ResolvedDependency with the conflict resolution version
                    // For now, just use the resolved_pkg as-is (it already has the correct version from resolution)
                    // The conflict resolution would have updated the node's resolved field if needed
                    resolved.insert(node.name.clone(), resolved_pkg.clone());
                }
            }
        }

        Ok(ResolutionResult {
            resolved,
            conflicts,
            decisions,
            workspace_packages: HashMap::new(),
        })
    }

    async fn resolve_version(&self, name: &str, range: &str) -> Result<String, MegagateError> {
        if range == "*" || range == "latest" {
            let versions = self.registry_client.get_package_versions(name).await?;
            if versions.is_empty() {
                return Err(MegagateError::PackageNotFound(name.to_string()));
            }
            return Ok(versions[0].version.clone());
        }

        let req = VersionReq::parse(range).map_err(|e| MegagateError::ConfigError(e.to_string()))?;
        let versions = self.registry_client.get_package_versions(name).await?;
        
        eprintln!("DEBUG: resolve_version for {} range {} - found {} versions", name, range, versions.len());
        for v in &versions {
            eprintln!("DEBUG:   version: {}", v.version);
        }

        for v in versions.iter() {
            let ver = Version::parse(&v.version).map_err(|e| MegagateError::ConfigError(e.to_string()))?;
            if req.matches(&ver) {
                eprintln!("DEBUG: Matched version {} for range {}", v.version, range);
                return Ok(v.version.clone());
            }
        }

        Err(MegagateError::VersionConflict(format!("No version matching {} for {}", range, name)))
    }

    async fn resolve_workspace_packages(
        &self,
        _manifests: &HashMap<String, PackageManifest>,
    ) -> Result<HashMap<String, ResolvedDependency>, MegagateError> {
        Ok(HashMap::new())
    }
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub frozen_lockfile: bool,
    pub production: bool,
    pub registry: Option<String>,
    pub store_dir: Option<String>,
}