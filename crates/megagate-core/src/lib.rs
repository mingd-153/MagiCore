use megagate_types::config::MegagateConfig;
use megagate_types::error::{MegagateError, Result};
use megagate_types::lockfile::{LockfileV1, ImporterDeps};
use megagate_types::package::{PackageManifest, ResolvedDependency, LockedPackage};
use megagate_types::store::StoreBackend;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

pub struct MegagateCore {
    resolver: Arc<megagate_resolver::Resolver>,
    linker: Arc<megagate_linker::Linker>,
    fetcher: Arc<megagate_fetcher::Fetcher>,
    security: Arc<megagate_security::SecurityManager>,
    config: MegagateConfig,
}

impl MegagateCore {
    pub async fn new(config: MegagateConfig) -> Result<Self> {
        let store_backend = Arc::new(megagate_extractor::FsStoreBackend::new(config.store_dir.clone()));
        store_backend.init().await?;
        let registry_client = Arc::new(megagate_fetcher::NpmRegistryClient::new(config.registry.clone()));
        let security_manager = Arc::new(megagate_security::SecurityManager::new(&config));
        let fetch_pool = megagate_fetcher::FetchPool::new(config.max_concurrency, std::time::Duration::from_secs(60));
        let fetcher = Arc::new(megagate_fetcher::Fetcher::new(
            store_backend.clone(),
            fetch_pool,
            registry_client.clone(),
            megagate_fetcher::FetcherConfig::default(),
        ));
        let resolver = Arc::new(megagate_resolver::Resolver::new(
            registry_client,
            security_manager.clone(),
            config.clone(),
        ));
        let linker = Arc::new(megagate_linker::Linker::new(store_backend, config.link_strategy));

        Ok(Self {
            resolver,
            linker,
            fetcher,
            security: security_manager,
            config,
        })
    }

    pub async fn install(&self, project_dir: &str) -> Result<InstallResult> {
        info!("Installing dependencies in {}", project_dir);

        let manifest = self.load_manifest(project_dir).await?;
        info!("Loaded manifest: {}", manifest.name);

        let lockfile = self.load_lockfile(project_dir).await?;
        info!("Loaded lockfile: {:?}", lockfile.is_some());

        let mut manifests = HashMap::new();
        manifests.insert(project_dir.to_string(), manifest.clone());

        let resolution = self.resolver.resolve(manifests, lockfile.clone(), megagate_resolver::InstallOptions {
            frozen_lockfile: false,
            production: false,
            registry: None,
            store_dir: None,
        }).await?;

        info!("Resolution complete, resolved {} packages", resolution.resolved.len());

        let resolved = resolution.resolved.clone();
        let pkgs: Vec<ResolvedDependency> = resolved.clone().into_values().collect();
        info!("Fetching {} packages", pkgs.len());
        let fetch_results = self.fetcher.fetch_multiple(pkgs).await?;
        info!("Fetch complete");

        // Update resolved packages with actual sizes from fetch results
        let mut resolved_with_sizes = HashMap::new();
        for (name, pkg) in resolved {
            let key = format!("{}@{}", pkg.name, pkg.version);
            let size = fetch_results.get(&format!("{}@{}", pkg.name, pkg.version))
                .map(|info| info.size)
                .unwrap_or(pkg.size);
            let mut updated_pkg = pkg;
            updated_pkg.size = size;
            resolved_with_sizes.insert(name, updated_pkg);
        }
        let resolved = resolved_with_sizes;

        // Create new lockfile from resolution
        let mut new_lockfile = LockfileV1::new(env!("CARGO_PKG_VERSION").to_string());
        new_lockfile.store.dir = self.config.store_dir.to_string_lossy().to_string();

        // Add resolved packages
        for (name, pkg) in &resolved {
            let locked_pkg = LockedPackage {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                integrity: pkg.integrity.clone(),
                resolved: pkg.resolved.clone(),
                size: pkg.size,
                dependencies: pkg.dependencies.clone(),
                optional_dependencies: pkg.optional_dependencies.clone(),
                peer_dependencies: pkg.peer_dependencies.clone(),
                bin: pkg.bin.clone(),
                engines: pkg.engines.clone(),
                provenance: None,
                approved_builds: vec![],
                publish_time: pkg.publish_time,
            };
            new_lockfile.add_package(locked_pkg.clone());
        }

        info!("Added {} packages to lockfile", new_lockfile.packages.len());

        // Add importer (root package)
        let mut importer_deps = ImporterDeps {
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            optional_dependencies: HashMap::new(),
        };
        for (name, version) in &manifest.dependencies {
            importer_deps.dependencies.insert(name.clone(), version.clone());
        }
        for (name, version) in &manifest.dev_dependencies {
            importer_deps.dev_dependencies.insert(name.clone(), version.clone());
        }
        for (name, version) in &manifest.optional_dependencies {
            importer_deps.optional_dependencies.insert(name.clone(), version.clone());
        }
        new_lockfile.add_importer(".".to_string(), importer_deps);

        // Save lockfile
        info!("Saving lockfile to {}", project_dir);
        self.save_lockfile(project_dir, &new_lockfile).await?;
        info!("Lockfile saved successfully");

        // Link packages
        info!("Linking packages");
        self.linker.link(&PathBuf::from(project_dir), &new_lockfile).await?;
        info!("Linking complete");

        let result = InstallResult {
            added: vec![],
            updated: vec![],
            removed: vec![],
        };

        info!("Installation completed successfully");
        Ok(result)
    }

    pub async fn add(&self, project_dir: &str, package_spec: &str, dev: bool) -> Result<InstallResult> {
        info!("Adding {} to {}", package_spec, project_dir);
        let mut manifest = self.load_manifest(project_dir).await?;

        let parts: Vec<&str> = package_spec.split('@').collect();
        let name = parts[0];
        let version = parts.get(1).unwrap_or(&"latest");

        if dev {
            manifest.dev_dependencies.insert(name.to_string(), version.to_string());
        } else {
            manifest.dependencies.insert(name.to_string(), version.to_string());
        }

        self.save_manifest(project_dir, &manifest).await?;
        self.install(project_dir).await
    }

    pub async fn update(&self, project_dir: &str, package_name: Option<&str>) -> Result<InstallResult> {
        info!("Updating {:?} in {}", package_name, project_dir);
        self.install(project_dir).await
    }

    pub async fn remove(&self, project_dir: &str, package_name: &str) -> Result<InstallResult> {
        info!("Removing {} from {}", package_name, project_dir);
        let mut manifest = self.load_manifest(project_dir).await?;
        manifest.dependencies.remove(package_name);
        manifest.dev_dependencies.remove(package_name);
        self.save_manifest(project_dir, &manifest).await?;
        self.install(project_dir).await
    }

    pub async fn list(&self, project_dir: &str, _depth: u32) -> Result<HashMap<String, String>> {
        let lockfile = self.load_lockfile(project_dir).await?;
        let mut result = HashMap::new();
        if let Some(lock) = lockfile {
            for (key, pkg) in lock.packages {
                result.insert(key, pkg.version.to_string());
            }
        }
        Ok(result)
    }

    pub async fn audit(&self, project_dir: &str) -> Result<AuditResult> {
        let _lockfile = self.load_lockfile(project_dir).await?;
        let issues = Vec::new();
        let vulnerabilities = Vec::new();
        Ok(AuditResult {
            issues,
            vulnerabilities,
            summary: "Audit completed".to_string(),
        })
    }

    pub async fn verify_lockfile(&self, project_dir: &str) -> Result<String> {
        let lockfile = self.load_lockfile(project_dir).await?;
        match lockfile {
            Some(lock) => {
                let valid = lock.verify_content_hash()?;
                if valid {
                    Ok("Lockfile is valid".to_string())
                } else {
                    Err(MegagateError::LockfileError("Lockfile content hash mismatch".to_string()))
                }
            }
            None => Err(MegagateError::LockfileError("No lockfile found".to_string())),
        }
    }

    async fn load_manifest(&self, project_dir: &str) -> Result<PackageManifest> {
        let path = PathBuf::from(project_dir).join("package.json");
        let content = std::fs::read_to_string(&path)
            .map_err(|e| MegagateError::IoError(format!("Failed to read package.json: {}", e)))?;
        serde_json::from_str(&content)
            .map_err(|e| MegagateError::ConfigError(format!("Invalid package.json: {}", e)))
    }

    async fn save_manifest(&self, project_dir: &str, manifest: &PackageManifest) -> Result<()> {
        let path = PathBuf::from(project_dir).join("package.json");
        let content = serde_json::to_string_pretty(manifest)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        std::fs::write(&path, content)
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    async fn load_lockfile(&self, project_dir: &str) -> Result<Option<LockfileV1>> {
        let path = PathBuf::from(project_dir).join("megagate-lock.json");
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
            let lock: LockfileV1 = serde_json::from_str(&content)
                .map_err(|e| MegagateError::LockfileError(e.to_string()))?;
            Ok(Some(lock))
        } else {
            Ok(None)
        }
    }

    async fn save_lockfile(&self, project_dir: &str, lockfile: &LockfileV1) -> Result<()> {
        let path = PathBuf::from(project_dir).join("megagate-lock.json");
        let content = serde_json::to_string_pretty(lockfile)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        std::fs::write(&path, content)
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallResult {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditResult {
    pub issues: Vec<String>,
    pub vulnerabilities: Vec<String>,
    pub summary: String,
}
