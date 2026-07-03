use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::{
    create_relative_symlink, validate_rel_path, LinkError, LinkResult, LinkerOptions,
    PackageLinkInfo, PackageLinkResult,
};

pub struct HoistedLinker {
    options: LinkerOptions,
}

impl HoistedLinker {
    pub fn new(options: LinkerOptions) -> Self {
        Self { options }
    }

    fn compute_peer_hash(&self, pkg: &PackageLinkInfo) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for (name, version) in &pkg.peer_dependencies {
            hasher.update(name.as_bytes());
            hasher.update(b"\0");
            hasher.update(version.as_bytes());
            hasher.update(b",");
        }
        hex::encode(hasher.finalize())[..8].to_string()
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
                .is_some_and(|g| g.matches(&pkg.name))
        })
    }

pub fn link_packages_internal(
        &self,
        packages: &[PackageLinkInfo],
        temp_dir: &Path,
    ) -> Result<LinkResult, LinkError> {
        let virtual_store = temp_dir.join("virtual_store");
        let mut linked = Vec::new();

        // Pre-compute all destination directories to batch create them
        let mut all_dirs = HashSet::new();

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

            // Ensure package directory exists even for empty packages
            all_dirs.insert(store_pkg_dir.clone());

            for (rel_path, _hash) in &pkg.files {
                if let Some(parent) = Path::new(rel_path).parent() {
                    all_dirs.insert(store_pkg_dir.join(parent));
                }
            }

            // Dep dirs
            let dep_dst_dir = virtual_store
                .join(&pkg_dir_name)
                .join("node_modules");
            all_dirs.insert(dep_dst_dir);
        }

        // Batch create all directories
        for dir in all_dirs {
            fs::create_dir_all(&dir)?;
        }

        // Now link files with parallel hardlinks
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

            // Collect all file link operations for parallel execution
            let link_ops: Vec<_> = pkg.files.iter().map(|(rel_path, hash)| {
                let src = self
                    .options
                    .store_path
                    .join("files")
                    .join("sha256")
                    .join(&hash[..2])
                    .join(hash);
                let dst = store_pkg_dir.join(rel_path);
                (src, dst)
            }).collect();

            // Parallel hardlinks
            link_ops.par_iter().try_for_each(|(src, dst)| {
                if self.options.symlinks {
                    fs::hard_link(src, dst).map_err(LinkError::Io)
                } else {
                    fs::copy(src, dst).map(|_| ()).map_err(LinkError::Io)
                }
            })?;

            if self.should_hoist(pkg) {
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
                validate_rel_path(dep_name)?;
                if let Some(ref ws) = self.options.workspace {
                    if let Some(ws_member) = ws.find_member(dep_name) {
                        let dep_dst_dir = virtual_store
                            .join(&pkg_dir_name)
                            .join("node_modules");
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

                    let dep_dst_dir = virtual_store
                        .join(&pkg_dir_name)
                        .join("node_modules");
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
            dep_graph_hash: String::new(),
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
            for (bin_name, bin_path) in &pkg.bin_entries {
                validate_rel_path(bin_name)?;
                validate_rel_path(bin_path)?;
                let src = virtual_store.join(format!(
                    "{}@{}_{}/node_modules/{}/{}",
                    pkg.name.replace(['/', '@'], "_"),
                    pkg.version,
                    self.compute_peer_hash(pkg),
                    pkg.name,
                    bin_path
                ));

                let dst = bin_dir.join(bin_name);
                create_relative_symlink(&src, &dst)?;
            }
        }

        Ok(())
    }
}

impl super::Linker for HoistedLinker {
    fn link_all(
        &self,
        packages: &[PackageLinkInfo],
        _store: &mgpm_store::store::cas::ContentStore,
        project_root: &Path,
    ) -> Result<LinkResult, LinkError> {
        let mgpm_dir = project_root.join(&self.options.virtual_store_dir);
        // Use a sibling temp dir at the same depth as mgpm_dir so that
        // relative symlinks computed during linking remain valid after rename
        let temp_dir = if let Some(parent) = mgpm_dir.parent() {
            let name = mgpm_dir.file_name().unwrap_or_default();
            parent.join(format!("{}.tmp_{}", name.to_string_lossy(), std::process::id()))
        } else {
            mgpm_dir.with_extension(format!("tmp_{}", std::process::id()))
        };
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).ok();
        }
        fs::create_dir_all(&temp_dir)?;

        let result = self.link_packages_internal(packages, &temp_dir);

        match &result {
            Ok(_) => {
                if mgpm_dir.exists() && !self.options.global_virtual_store {
                    fs::remove_dir_all(&mgpm_dir).ok();
                }
                let mgpm_parent = mgpm_dir.parent().unwrap_or(project_root);
                fs::create_dir_all(mgpm_parent)?;
                if mgpm_dir.exists() {
                    fs::remove_dir_all(&mgpm_dir).ok();
                }
                fs::rename(&temp_dir, &mgpm_dir)?;

                // Create pnpm-style node_modules -> .mgpm symlink at project root
                let root_node_modules = project_root.join("node_modules");
                if !root_node_modules.exists() {
                    fs::create_dir_all(&root_node_modules)?;
                }
                let mgpm_link = root_node_modules.join(".mgpm");
                if !mgpm_link.exists() {
                    create_relative_symlink(&mgpm_dir, &mgpm_link)?;
                }

                // Hoist packages into project-level node_modules
                let hoisted_source = mgpm_dir.join("node_modules");
                if hoisted_source.exists() {
                    if let Ok(entries) = fs::read_dir(&hoisted_source) {
                        for entry in entries.flatten() {
                            let name = entry.file_name();
                            let hoist_dst = root_node_modules.join(&name);
                            if name != ".bin" && name != ".mgpm" && !hoist_dst.exists() {
                                create_relative_symlink(&entry.path(), &hoist_dst)?;
                            }
                        }
                    }
                }

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

    fn link_package(
        &self,
        pkg: &PackageLinkInfo,
        _store: &mgpm_store::store::cas::ContentStore,
        dest: &Path,
    ) -> Result<(), LinkError> {
        fs::create_dir_all(dest)?;

        for (rel_path, hash) in &pkg.files {
            validate_rel_path(rel_path)?;
            let src = self
                .options
                .store_path
                .join("files")
                .join("sha256")
                .join(&hash[..2])
                .join(hash);
 
            let dst = dest.join(rel_path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }

            if self.options.symlinks {
                fs::hard_link(&src, &dst)?;
            } else {
                fs::copy(&src, &dst)?;
            }
        }

        for dep_name in &pkg.dependencies {
            validate_rel_path(dep_name)?;
            let dep_src = dest
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join(dep_name))
                .unwrap_or_else(|| PathBuf::from("..").join(dep_name));
            let dep_dst = dest.join("node_modules").join(dep_name);

            if !dep_dst.exists() {
                if let Some(parent) = dep_dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                create_relative_symlink(&dep_src, &dep_dst)?;
            }
        }

        Ok(())
    }

    fn link_bins(
        &self,
        packages: &[PackageLinkInfo],
        _store: &mgpm_store::store::cas::ContentStore,
        bin_dir: &Path,
    ) -> Result<(), LinkError> {
        fs::create_dir_all(bin_dir)?;

        for pkg in packages {
            for (bin_name, bin_path) in &pkg.bin_entries {
                validate_rel_path(bin_name)?;
                validate_rel_path(bin_path)?;
                let src = PathBuf::from("..")
                    .join(".mgpm")
                    .join(&pkg.name)
                    .join(bin_path);
                let dst = bin_dir.join(bin_name);

                create_relative_symlink(&src, &dst)?;
            }
        }

        Ok(())
    }

    fn unlink_package(&self, name: &str, project_root: &Path) -> Result<(), LinkError> {
        let dep_link = project_root.join("node_modules").join(name);
        if dep_link.exists() {
            fs::remove_file(&dep_link)?;
        }
        Ok(())
    }

    fn strategy(&self) -> super::LinkerStrategy {
        super::LinkerStrategy::Hoisted
    }
}
