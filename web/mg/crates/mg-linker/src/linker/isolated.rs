use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use blake3;
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use super::{
    create_relative_symlink, validate_rel_path, LinkError, LinkResult, LinkerOptions,
    PackageLinkInfo, PackageLinkResult,
};

pub struct IsolatedLinker {
    gvs: mg_store::store::gvs::GlobalVirtualStore,
    options: LinkerOptions,
}

impl IsolatedLinker {
    pub fn new(gvs: mg_store::store::gvs::GlobalVirtualStore, options: LinkerOptions) -> Self {
        Self { gvs, options }
    }

    fn compute_peer_hash(peer_deps: &[(String, String)]) -> String {
        let mut hasher = Sha256::new();
        for (name, version) in peer_deps {
            hasher.update(name.as_bytes());
            hasher.update(b"\0");
            hasher.update(version.as_bytes());
            hasher.update(b",");
        }
        hex::encode(hasher.finalize())[..8].to_string()
    }

    fn compute_dep_graph_hash(packages: &[PackageLinkInfo]) -> String {
        let mut hasher = blake3::Hasher::new();
        let mut sorted = packages.to_vec();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));

        for pkg in &sorted {
            hasher.update(pkg.name.as_bytes());
            hasher.update(b"\0");
            hasher.update(pkg.version.as_bytes());
            hasher.update(b"\0");
            for dep in &pkg.dependencies {
                hasher.update(dep.as_bytes());
                hasher.update(b",");
            }
            hasher.update(b"\0");
        }

        hasher.finalize().to_hex().to_string()
    }

    fn pkg_dir_name(pkg: &PackageLinkInfo) -> String {
        let suffix = format!("{}@{}", pkg.name, pkg.version)
            .replace('/', "_")
            .replace('@', "_");
        let peer_hash = Self::compute_peer_hash(&pkg.peer_dependencies);
        format!("{}_{}", suffix, peer_hash)
    }

    fn make_integrity_hash(hash: &str) -> mg_store::store::cas::IntegrityHash {
        mg_store::store::cas::IntegrityHash {
            hash: hash.to_string(),
            shard: hash[..2].to_string(),
            filename: hash.to_string(),
            is_executable: false,
        }
    }

    fn link_package_internal(
        pkg: &PackageLinkInfo,
        store_path: &Path,
        pkg_dir: &Path,
    ) -> Result<(), LinkError> {
        fs::create_dir_all(pkg_dir)?;

        // Hardlink files from CAS store
        for (rel_path, hash) in &pkg.files {
            validate_rel_path(rel_path)?;
            let src = store_path
                .join("files")
                .join("sha256")
                .join(&hash[..2])
                .join(hash);
            let dst = pkg_dir.join(rel_path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            if !dst.exists() {
                fs::hard_link(&src, &dst).or_else(|_| fs::copy(&src, &dst).map(|_| ()))?;
            }
        }
        Ok(())
    }
}

impl super::Linker for IsolatedLinker {
    fn link_all(
        &self,
        packages: &[PackageLinkInfo],
        _store: &mg_store::store::cas::ContentStore,
        project_root: &Path,
    ) -> Result<LinkResult, LinkError> {
        let dep_graph_hash = Self::compute_dep_graph_hash(packages);

        self.gvs
            .ensure_dirs()
            .map_err(|e| LinkError::Other(e.to_string()))?;

        let mg_dir = project_root.join(&self.options.virtual_store_dir);

        // Use a sibling temp dir for atomic rename
        let temp_dir = if let Some(parent) = mg_dir.parent() {
            let name = mg_dir.file_name().unwrap_or_default();
            parent.join(format!("{}.tmp_{}", name.to_string_lossy(), std::process::id()))
        } else {
            mg_dir.with_extension(format!("tmp_{}", std::process::id()))
        };
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).ok();
        }

        let virtual_store = temp_dir.join("virtual_store");

        // Pre-compute all dirs for batch creation
        let mut all_dirs = HashSet::new();
        for pkg in packages {
            let dir_name = Self::pkg_dir_name(pkg);
            let pkg_dest = virtual_store
                .join(&dir_name)
                .join("node_modules")
                .join(&pkg.name);
            all_dirs.insert(pkg_dest);
            for (rel_path, _hash) in &pkg.files {
                if let Some(parent) = Path::new(rel_path).parent() {
                    let dir = virtual_store
                        .join(&dir_name)
                        .join("node_modules")
                        .join(&pkg.name)
                        .join(parent);
                    all_dirs.insert(dir);
                }
            }
            // Dep node_modules dir
            all_dirs.insert(
                virtual_store
                    .join(&dir_name)
                    .join("node_modules"),
            );
        }
        for dir in &all_dirs {
            fs::create_dir_all(dir)?;
        }

        // Link files in parallel
        let ops: Vec<_> = packages.iter().map(|pkg| {
            let dir_name = Self::pkg_dir_name(pkg);
            let pkg_dest = virtual_store
                .join(&dir_name)
                .join("node_modules")
                .join(&pkg.name);
            let store_path = self.options.store_path.clone();
            (pkg, pkg_dest, store_path)
        }).collect();

        ops.par_iter().try_for_each(|(pkg, pkg_dest, store_path)| {
            Self::link_package_internal(pkg, store_path, pkg_dest)
        })?;

        // Create dep symlinks within each package's node_modules
        for pkg in packages {
            let dir_name = Self::pkg_dir_name(pkg);
            let pkg_node_modules = virtual_store
                .join(&dir_name)
                .join("node_modules");

            for dep_name in &pkg.dependencies {
                validate_rel_path(dep_name)?;
                if let Some(dep_pkg) = packages.iter().find(|p| p.name == *dep_name) {
                    let dep_dir_name = Self::pkg_dir_name(dep_pkg);
                    let dep_src = virtual_store
                        .join(&dep_dir_name)
                        .join("node_modules")
                        .join(&dep_pkg.name);
                    let dep_dst = pkg_node_modules.join(dep_name);
                    if !dep_dst.exists() && dep_src.exists() {
                        create_relative_symlink(&dep_src, &dep_dst)?;
                    }
                }
            }
        }

        // Rename temp to .mg atomically
        if mg_dir.exists() {
            fs::remove_dir_all(&mg_dir).ok();
        }
        if let Some(parent) = mg_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&temp_dir, &mg_dir)?;

        // Create node_modules/.mg -> ../.mg symlink
        let node_modules = project_root.join("node_modules");
        fs::create_dir_all(&node_modules)?;
        let mg_link = node_modules.join(".mg");
        if !mg_link.exists() {
            create_relative_symlink(&mg_dir, &mg_link)?;
        }

        // Create symlinks for direct deps in root node_modules
        let virtual_store_real = mg_dir.join("virtual_store");
        for pkg in packages.iter().filter(|p| p.is_root_dep) {
            let dir_name = Self::pkg_dir_name(pkg);
            let src = virtual_store_real
                .join(&dir_name)
                .join("node_modules")
                .join(&pkg.name);
            let dst = node_modules.join(&pkg.name);
            if !dst.exists() && src.exists() {
                create_relative_symlink(&src, &dst)?;
            }
        }

        // Bin symlinks
        let bin_dir = node_modules.join(".bin");
        for pkg in packages {
            for (bin_name, bin_path) in &pkg.bin_entries {
                validate_rel_path(bin_name)?;
                validate_rel_path(bin_path)?;
                let dir_name = Self::pkg_dir_name(pkg);
                let src = virtual_store_real
                    .join(&dir_name)
                    .join("node_modules")
                    .join(&pkg.name)
                    .join(bin_path);
                let dst = bin_dir.join(bin_name);
                if src.exists() && !dst.exists() {
                    if let Some(parent) = dst.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    create_relative_symlink(&src, &dst)?;
                }
            }
        }

        // Refcount callback
        if let Some(ref callback) = self.options.refcount_callback {
            for pkg in packages {
                let package_id = format!("{}@{}", pkg.name, pkg.version);
                callback(&package_id)?;
            }
        }

        let linked_results: Vec<PackageLinkResult> = packages
            .iter()
            .map(|p| {
                let dir_name = Self::pkg_dir_name(p);
                PackageLinkResult {
                    name: p.name.clone(),
                    version: p.version.clone(),
                    path: virtual_store_real
                        .join(&dir_name)
                        .join("node_modules")
                        .join(&p.name),
                    peer_hash: Self::compute_peer_hash(&p.peer_dependencies),
                    linked_deps: p.dependencies.clone(),
                }
            })
            .collect();

        Ok(LinkResult {
            linked: linked_results,
            node_modules_path: node_modules,
            dep_graph_hash,
        })
    }

    fn link_package(
        &self,
        pkg: &PackageLinkInfo,
        store: &mg_store::store::cas::ContentStore,
        dest: &Path,
    ) -> Result<(), LinkError> {
        fs::create_dir_all(dest)?;

        for (rel_path, hash) in &pkg.files {
            validate_rel_path(rel_path)?;
            let integrity_hash = Self::make_integrity_hash(hash);
            let src = integrity_hash.cas_path(store.root());

            let dst = dest.join(rel_path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            if !dst.exists() {
                fs::hard_link(&src, &dst).or_else(|_| fs::copy(&src, &dst).map(|_| ()))?;
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
        _store: &mg_store::store::cas::ContentStore,
        bin_dir: &Path,
    ) -> Result<(), LinkError> {
        fs::create_dir_all(bin_dir)?;

        for pkg in packages {
            for (bin_name, bin_path) in &pkg.bin_entries {
                validate_rel_path(bin_name)?;
                validate_rel_path(bin_path)?;
                let dir_name = Self::pkg_dir_name(pkg);
                let src = PathBuf::from("..")
                    .join(".mg")
                    .join("virtual_store")
                    .join(&dir_name)
                    .join("node_modules")
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
        super::LinkerStrategy::Isolated
    }
}
