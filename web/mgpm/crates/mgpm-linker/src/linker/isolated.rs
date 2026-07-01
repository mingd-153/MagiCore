use std::fs;
use std::path::{Path, PathBuf};

use blake3;

use super::{
    create_relative_symlink, validate_rel_path, LinkError, LinkResult, LinkerOptions,
    PackageLinkInfo, PackageLinkResult,
};

pub struct IsolatedLinker {
    gvs: mgpm_store::store::gvs::GlobalVirtualStore,
    #[allow(dead_code)]
    options: LinkerOptions,
}

impl IsolatedLinker {
    pub fn new(gvs: mgpm_store::store::gvs::GlobalVirtualStore, options: LinkerOptions) -> Self {
        Self { gvs, options }
    }

    fn virtual_store_path(project_root: &Path, dep_graph_hash: &str) -> PathBuf {
        project_root
            .join("node_modules")
            .join(".mgpm")
            .join(dep_graph_hash)
            .join("node_modules")
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

    fn make_integrity_hash(hash: &str) -> mgpm_store::store::cas::IntegrityHash {
        mgpm_store::store::cas::IntegrityHash {
            hash: hash.to_string(),
            shard: hash[..2].to_string(),
            filename: hash.to_string(),
            is_executable: false,
        }
    }
}

impl super::Linker for IsolatedLinker {
    fn link_all(
        &self,
        packages: &[PackageLinkInfo],
        _store: &mgpm_store::store::cas::ContentStore,
        project_root: &Path,
    ) -> Result<LinkResult, LinkError> {
        let dep_graph_hash = Self::compute_dep_graph_hash(packages);

        self.gvs
            .ensure_dirs()
            .map_err(|e| LinkError::Other(e.to_string()))?;

        let node_modules = project_root.join("node_modules");
        fs::create_dir_all(&node_modules)?;

        let direct_deps: Vec<String> = packages
            .iter()
            .filter(|p| p.is_root_dep)
            .map(|p| p.name.clone())
            .collect();

        let vs_path = Self::virtual_store_path(project_root, &dep_graph_hash);
        for dep_name in &direct_deps {
            let src = vs_path.join(dep_name);
            let dst = node_modules.join(dep_name);
            if !dst.exists() {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                if src.exists() {
                    create_relative_symlink(&src, &dst)?;
                }
            }
        }

        let bin_dir = node_modules.join(".bin");
        self.create_bin_symlinks(packages, &vs_path, &bin_dir)?;

        let linked_results: Vec<PackageLinkResult> = packages
            .iter()
            .map(|p| PackageLinkResult {
                name: p.name.clone(),
                version: p.version.clone(),
                path: vs_path.join(&p.name),
                peer_hash: String::new(),
                linked_deps: p.dependencies.clone(),
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
        store: &mgpm_store::store::cas::ContentStore,
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
                create_relative_symlink(&src, &dst)?;
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
                let dep_graph_hash = Self::compute_dep_graph_hash(std::slice::from_ref(pkg));

                let src = PathBuf::from("..")
                    .join(".mgpm")
                    .join(&dep_graph_hash)
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

impl IsolatedLinker {
    fn create_bin_symlinks(
        &self,
        packages: &[PackageLinkInfo],
        vs_path: &Path,
        bin_dir: &Path,
    ) -> Result<(), LinkError> {
        fs::create_dir_all(bin_dir)?;

        for pkg in packages {
            for (bin_name, bin_path) in &pkg.bin_entries {
                validate_rel_path(bin_name)?;
                validate_rel_path(bin_path)?;
                let src = vs_path.join(&pkg.name).join(bin_path);
                let dst = bin_dir.join(bin_name);

                if src.exists() && !dst.exists() {
                    create_relative_symlink(&src, &dst)?;
                }
            }
        }

        Ok(())
    }
}
