//! Strict Linker - node_modules Generator

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LinkerOptions {
    pub project_root: PathBuf,
    pub virtual_store_dir: PathBuf,
    pub global_virtual_store: bool,
    pub hoist: bool,
}

impl Default for LinkerOptions {
    fn default() -> Self {
        Self {
            project_root: PathBuf::from("."),
            virtual_store_dir: PathBuf::from(".mgpm"),
            global_virtual_store: false,
            hoist: false,
        }
    }
}

pub struct Linker {
    options: LinkerOptions,
}

impl Linker {
    pub fn new(options: LinkerOptions) -> Self {
        Self { options }
    }

    pub fn link_packages(&self, packages: &[PackageLinkInfo]) -> Result<LinkResult, LinkError> {
        let temp_dir = self.options.project_root.join(".mgpm_temp");
        if temp_dir.exists() { fs::remove_dir_all(&temp_dir).ok(); }
        fs::create_dir_all(&temp_dir)?;

        let result = self.link_packages_internal(packages, &temp_dir);

        if result.is_ok() {
            let mgpm_dir = self.options.project_root.join(&self.options.virtual_store_dir);
            if mgpm_dir.exists() && !self.options.global_virtual_store {
                fs::remove_dir_all(&mgpm_dir).ok();
            }
            fs::rename(&temp_dir, &mgpm_dir).ok();
        } else {
            fs::remove_dir_all(&temp_dir).ok();
        }
        result
    }

    fn link_packages_internal(&self, packages: &[PackageLinkInfo], temp_dir: &Path) -> Result<LinkResult, LinkError> {
        let mut linked = Vec::new();
        for pkg in packages {
            let pkg_dir_name = format!("{}_{}", pkg.name.replace("@", "_").replace("/", "_"), self.compute_peer_hash(pkg));
            let target_dir = temp_dir.join(&pkg_dir_name);
            fs::create_dir_all(&target_dir)?;
            let node_modules = target_dir.join("node_modules");
            fs::create_dir_all(&node_modules)?;
            linked.push(PackageLinkResult { name: pkg.name.clone(), version: pkg.version.clone(), path: target_dir, peer_hash: self.compute_peer_hash(pkg) });
        }
        Ok(LinkResult { linked, node_modules_path: temp_dir.join("node_modules") })
    }

    fn compute_peer_hash(&self, pkg: &PackageLinkInfo) -> String {
        if pkg.peer_dependencies.is_empty() { return String::new(); }
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut sorted: Vec<_> = pkg.peer_dependencies.iter().collect();
        sorted.sort();
        let mut hasher = DefaultHasher::new();
        for (n, v) in sorted { n.hash(&mut hasher); v.hash(&mut hasher); }
        format!("{:x}", hasher.finish())
    }
}

#[derive(Debug, Clone)]
pub struct PackageLinkInfo {
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
    pub peer_dependencies: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct LinkResult {
    pub linked: Vec<PackageLinkResult>,
    pub node_modules_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PackageLinkResult {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub peer_hash: String,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum LinkError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
