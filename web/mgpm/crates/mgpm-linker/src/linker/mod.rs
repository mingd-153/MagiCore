use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

pub type RefcountCallback = Arc<dyn Fn(&str) -> io::Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct LinkerOptions {
    pub project_root: PathBuf,
    pub virtual_store_dir: PathBuf,
    pub global_virtual_store: bool,
    pub hoist: bool,
    pub hoist_pattern: Vec<String>,
    pub symlinks: bool,
    pub store_path: PathBuf,
    pub refcount_callback: Option<RefcountCallback>,
    pub workspace: Option<mgpm_workspace::Workspace>,
}

impl std::fmt::Debug for LinkerOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkerOptions")
            .field("project_root", &self.project_root)
            .field("virtual_store_dir", &self.virtual_store_dir)
            .field("global_virtual_store", &self.global_virtual_store)
            .field("hoist", &self.hoist)
            .field("hoist_pattern", &self.hoist_pattern)
            .field("symlinks", &self.symlinks)
            .field("store_path", &self.store_path)
            .field("refcount_callback", &self.refcount_callback.as_ref().map(|_| "Box<dyn Fn>"))
            .field("workspace", &self.workspace)
            .finish()
    }
}

impl Default for LinkerOptions {
    fn default() -> Self {
        Self {
            project_root: PathBuf::from("."),
            virtual_store_dir: PathBuf::from(".mgpm"),
            global_virtual_store: false,
            hoist: false,
            hoist_pattern: vec!["*".to_string()],
            symlinks: true,
            store_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mgpm")
                .join("store"),
            refcount_callback: None,
            workspace: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageLinkInfo {
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
    pub peer_dependencies: Vec<(String, String)>,
    pub files: Vec<(String, String)>,
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
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).ok();
        }
        fs::create_dir_all(&temp_dir)?;

        let result = self.link_packages_internal(packages, &temp_dir);

        match &result {
            Ok(_) => {
                let mgpm_dir =
                    self.options.project_root.join(&self.options.virtual_store_dir);
                if mgpm_dir.exists() && !self.options.global_virtual_store {
                    fs::remove_dir_all(&mgpm_dir).ok();
                }
                let mgpm_parent = mgpm_dir.parent().unwrap_or(&self.options.project_root);
                fs::create_dir_all(mgpm_parent)?;
                if mgpm_dir.exists() {
                    fs::remove_dir_all(&mgpm_dir).ok();
                }
                fs::rename(&temp_dir, &mgpm_dir)?;

                if let Some(ref callback) = self.options.refcount_callback {
                    for pkg in packages {
                        let package_id = format!("{}@{}", pkg.name, pkg.version);
                        callback(&package_id)?;
                    }
                }
            }
            Err(_) => {
                fs::remove_dir_all(&temp_dir).ok();
            }
        }

        result
    }

    fn link_packages_internal(
        &self,
        packages: &[PackageLinkInfo],
        temp_dir: &Path,
    ) -> Result<LinkResult, LinkError> {
        let virtual_store = temp_dir.join("virtual_store");
        let mut linked = Vec::new();

        for pkg in packages {
            let peer_hash = self.compute_peer_hash(pkg);
            let dir_suffix = format!("{}@{}", pkg.name, pkg.version)
                .replace('/', "_")
                .replace('@', "_");
            let pkg_dir_name = format!("{}_{}", dir_suffix, peer_hash);

            let store_pkg_dir = virtual_store
                .join(&pkg_dir_name)
                .join("node_modules")
                .join(&pkg.name);
            fs::create_dir_all(&store_pkg_dir)?;

            for (rel_path, hash) in &pkg.files {
                let src = self
                    .options
                    .store_path
                    .join("files")
                    .join("sha256")
                    .join(&hash[..2])
                    .join(hash);

                let dst = store_pkg_dir.join(rel_path);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }

                if self.options.symlinks {
                    create_relative_symlink(&src, &dst)?;
                } else {
                    fs::copy(&src, &dst)?;
                }
            }

            if self.options.hoist && self.should_hoist(pkg) {
                let hoist_node_modules = temp_dir.join("node_modules");
                fs::create_dir_all(&hoist_node_modules)?;

                let hoist_dst = hoist_node_modules.join(&pkg.name);
                if !hoist_dst.exists() {
                    create_relative_symlink(&store_pkg_dir, &hoist_dst)?;
                }
            }

            let mgpm_root = temp_dir.join(".mgpm");
            fs::create_dir_all(&mgpm_root)?;

            let pkg_link = mgpm_root.join(&pkg.name);
            create_relative_symlink(&store_pkg_dir, &pkg_link)?;

            for dep_name in &pkg.dependencies {
                // Check if the dependency is a workspace member first.
                // If so, symlink directly to the workspace member's directory
                // for real-time editing during development.
                if let Some(ref ws) = self.options.workspace {
                    if let Some(ws_member) = ws.find_member(dep_name) {
                        let dep_dst_dir = store_pkg_dir.join("node_modules");
                        fs::create_dir_all(&dep_dst_dir)?;
                        let dep_dst = dep_dst_dir.join(dep_name);
                        if !dep_dst.exists() && ws_member.path.exists() {
                            create_relative_symlink(&ws_member.path, &dep_dst)?;
                        }
                        continue;
                    }
                }

                if let Some(dep_pkg) = packages.iter().find(|p| p.name == *dep_name) {
                    let dep_peer_hash = self.compute_peer_hash(dep_pkg);
                    let dep_suffix = format!("{}@{}", dep_pkg.name, dep_pkg.version)
                        .replace('/', "_")
                        .replace('@', "_");
                    let dep_dir_name = format!("{}_{}", dep_suffix, dep_peer_hash);
                    let dep_src = virtual_store
                        .join(&dep_dir_name)
                        .join("node_modules")
                        .join(&dep_pkg.name);

                    let dep_dst_dir = store_pkg_dir.join("node_modules");
                    fs::create_dir_all(&dep_dst_dir)?;
                    let dep_dst = dep_dst_dir.join(&dep_pkg.name);

                    if !dep_dst.exists() && dep_src.exists() {
                        create_relative_symlink(&dep_src, &dep_dst)?;
                    }
                }
            }

            linked.push(PackageLinkResult {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                path: store_pkg_dir,
                peer_hash: peer_hash.clone(),
                linked_deps: pkg.dependencies.clone(),
            });
        }

        let root_node_modules = if self.options.hoist {
            temp_dir.join("node_modules")
        } else {
            let nm = temp_dir.join("node_modules");
            fs::create_dir_all(&nm)?;

            let mgpm_root = temp_dir.join(".mgpm");
            let virtual_store_link = nm.join(".mgpm");
            if !virtual_store_link.exists() {
                create_relative_symlink(&mgpm_root, &virtual_store_link)?;
            }

            nm
        };

        self.create_bin_symlinks(packages, &virtual_store, &root_node_modules)?;

        Ok(LinkResult {
            linked,
            node_modules_path: root_node_modules,
        })
    }

    fn should_hoist(&self, pkg: &PackageLinkInfo) -> bool {
        if !self.options.hoist {
            return false;
        }
        if self.options.hoist_pattern.is_empty() {
            return true;
        }
        if self.options.hoist_pattern.contains(&"*".to_string()) {
            return true;
        }
        self.options.hoist_pattern.iter().any(|pat| {
            glob::Pattern::new(pat)
                .ok()
                .map_or(false, |g| g.matches(&pkg.name))
        })
    }

    fn create_bin_symlinks(
        &self,
        packages: &[PackageLinkInfo],
        virtual_store: &Path,
        node_modules: &Path,
    ) -> Result<(), LinkError> {
        let bin_dir = node_modules.join(".bin");
        fs::create_dir_all(&bin_dir)?;

        for pkg in packages {
            let peer_hash = self.compute_peer_hash(pkg);
            let dir_suffix = format!("{}@{}", pkg.name, pkg.version)
                .replace('/', "_")
                .replace('@', "_");
            let pkg_dir_name = format!("{}_{}", dir_suffix, peer_hash);
            let pkg_node_modules = virtual_store
                .join(&pkg_dir_name)
                .join("node_modules")
                .join(&pkg.name);

            let package_json_path = pkg_node_modules.join("package.json");
            if !package_json_path.exists() {
                continue;
            }

            let content = match fs::read_to_string(&package_json_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let json: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let bin = match json.get("bin") {
                Some(b) => b,
                None => continue,
            };

            let bin_entries: Vec<(String, String)> = if let Some(obj) = bin.as_object() {
                obj.iter()
                    .map(|(k, v)| {
                        let path = v.as_str().unwrap_or(k);
                        (k.clone(), path.to_string())
                    })
                    .collect()
            } else if let Some(s) = bin.as_str() {
                let name = pkg
                    .name
                    .split('/')
                    .last()
                    .unwrap_or(&pkg.name)
                    .to_string();
                vec![(name, s.to_string())]
            } else {
                continue;
            };

            for (bin_name, bin_path) in &bin_entries {
                let src = pkg_node_modules.join(bin_path);
                let dst = bin_dir.join(bin_name);

                if src.exists() && !dst.exists() {
                    create_relative_symlink(&src, &dst)?;
                }
            }
        }

        Ok(())
    }

    fn compute_peer_hash(&self, pkg: &PackageLinkInfo) -> String {
        if pkg.peer_dependencies.is_empty() {
            return String::new();
        }
        let mut sorted: Vec<_> = pkg.peer_dependencies.iter().collect();
        sorted.sort();
        let mut hasher = Sha256::new();
        for (n, v) in sorted {
            hasher.update(n.as_bytes());
            hasher.update(b"\0");
            hasher.update(v.as_bytes());
            hasher.update(b"\0");
        }
        hex::encode(hasher.finalize())[..8].to_string()
    }
}

fn create_relative_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        return Ok(());
    }

    let relative = make_relative(dst, src)?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&relative, dst)?;
    }
    #[cfg(not(unix))]
    {
        if relative.is_dir() {
            std::os::windows::fs::symlink_dir(&relative, dst)?;
        } else {
            std::os::windows::fs::symlink_file(&relative, dst)?;
        }
    }
    Ok(())
}

fn make_relative(base: &Path, target: &Path) -> io::Result<PathBuf> {
    let abs_base = if base.is_absolute() {
        base.to_path_buf()
    } else {
        std::env::current_dir()?.join(base)
    };
    let abs_target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()?.join(target)
    };

    let base_components: Vec<_> = abs_base.components().collect();
    let target_components: Vec<_> = abs_target.components().collect();

    let common_len = base_components
        .iter()
        .zip(target_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = PathBuf::new();

    if common_len >= base_components.len() {
        for component in &target_components[common_len..] {
            result.push(component);
        }
        return Ok(result);
    }

    for _ in common_len..base_components.len().saturating_sub(1) {
        result.push("..");
    }

    for component in &target_components[common_len..] {
        result.push(component);
    }

    Ok(result)
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
    pub linked_deps: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
