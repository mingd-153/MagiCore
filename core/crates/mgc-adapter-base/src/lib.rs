#![cfg_attr(test, allow(clippy::unwrap_used))]

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use mgc_types::adapter::{AddOptions, InstalledPackage, PackageAdapter, UpdatedPackage};
use mgc_types::error::MgResult;
use mgc_types::package::{DependencySpec, PackageId, PackageName, VersionRange};
use mgc_types::version::Version;

/// Cargo.toml parse/write helpers — shared by lib (rust), game (bevy), iot (esp32-rust) cores.
pub mod cargo_manifest;

/// BaseAdapter — default implementations for add/remove/list/update.
///
/// Each ecosystem adapter must implement both `PackageAdapter` and `BaseAdapter`.
/// A blanket impl (`impl<T> BaseAdapter for T where T: PackageAdapter`) is not
/// possible because Rust's orphan rules prevent adding foreign trait methods
/// to foreign types. So each adapter explicitly calls self.base_*() in their
/// PackageAdapter impls.
///
/// #  Lifecycle
///    base_add  = parse_manifest → mutate → write_manifest (no fs lock yet)
///    base_remove = parse_manifest → mutate → write_manifest
///    base_list = parse_manifest → read-only
///    base_update = placeholder (returns empty)
///
/// #  TOCTOU warning
///   These methods are NOT atomic. Concurrent `mgc add` + `mgc remove` on the
///   same project will race on read-modify-write. A file-level advisory lock
///   (.magicore/.lock) is planned but not yet implemented.
#[async_trait]
pub trait BaseAdapter: PackageAdapter + Send + Sync {
    /// Root directory where installed dependencies are materialized for this
    /// adapter. Adapters can override this when they do not use
    /// `node_modules`-style layouts.
    fn install_root(&self, project_root: &Path) -> PathBuf {
        project_root.join("node_modules")
    }

    fn normalize_range(range: Option<&VersionRange>, exact: bool) -> Option<VersionRange> {
        range.map(|r| {
            if exact {
                let s = r.as_str().trim_start_matches('^').trim_start_matches('~');
                VersionRange::parse(s).unwrap_or_else(|_| r.clone())
            } else {
                r.clone()
            }
        })
    }

    async fn base_add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId> {
        let mut manifest = self.parse_manifest(project_root).await?;
        let range = Self::normalize_range(range, opts.exact);
        let mut spec = DependencySpec::new(
            name.clone(),
            range.clone().unwrap_or_else(VersionRange::star),
        );
        spec.dev = opts.dev;
        spec.optional = opts.optional;
        spec.peer = opts.peer;
        manifest.add_dep(spec, opts.dev, opts.optional, opts.peer);
        if !opts.no_save {
            self.write_manifest(project_root, &manifest).await?;
        }
        let ver = range
            .as_ref()
            .and_then(|r| r.satisfying_version())
            .unwrap_or_else(|| Version::new(0, 0, 0));
        Ok(PackageId::new(name.clone(), ver))
    }

    async fn base_remove(
        &self,
        project_root: &Path,
        name: &PackageName,
    ) -> Result<(), mgc_types::error::MgError> {
        let mut manifest = self.parse_manifest(project_root).await?;
        manifest.remove_dep(name.as_str());
        self.write_manifest(project_root, &manifest).await?;
        Ok(())
    }

    async fn base_list(
        &self,
        project_root: &Path,
    ) -> Result<Vec<InstalledPackage>, mgc_types::error::MgError> {
        let manifest = self.parse_manifest(project_root).await?;
        let mut packages = Vec::new();
        // Use the group label string to detect dev deps — NOT pointer comparison
        // (as_ptr() on empty Vecs may alias in Rust, causing false positives).
        for (label, deps) in manifest.dep_groups() {
            let is_dev = label == "devDependencies";
            let install_root = self.install_root(project_root);
            for dep in deps {
                packages.push(InstalledPackage {
                    id: PackageId::new(dep.name.clone(), Version::new(0, 0, 0)),
                    path: install_root.join(dep.name.as_str()),
                    integrity: None,
                    is_direct: true,
                    is_dev,
                });
            }
        }
        Ok(packages)
    }

    async fn base_update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> Result<Vec<UpdatedPackage>, mgc_types::error::MgError> {
        Ok(Vec::new())
    }
}

