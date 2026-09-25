#![cfg_attr(test, allow(clippy::unwrap_used))]

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use mgc_types::adapter::{AddOptions, InstalledPackage, PackageAdapter, UpdatedPackage};
use mgc_types::error::MgResult;
use mgc_types::package::{PackageId, PackageName, VersionRange};

/// Cargo.toml parse/write helpers — shared by lib (rust), game (bevy), iot (esp32-rust) cores.
pub mod cargo_manifest;

/// BaseAdapter — safe shared manifest removal helper.
///
/// Each ecosystem adapter must implement both `PackageAdapter` and `BaseAdapter`.
/// A blanket impl (`impl<T> BaseAdapter for T where T: PackageAdapter`) is not
/// possible because Rust's orphan rules prevent adding foreign trait methods
/// to foreign types. So each adapter explicitly calls self.base_*() in their
/// PackageAdapter impls.
///
/// #  Lifecycle
///    base_remove = parse_manifest → mutate → write_manifest
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
        _project_root: &Path,
        _name: &PackageName,
        _range: Option<&VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        Err(mgc_types::capabilities::unsupported_capability(
            "adapter",
            "generic add",
            "use an ecosystem-specific resolver and manifest writer; this fallback cannot claim a resolved version",
        ))
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
        _project_root: &Path,
    ) -> Result<Vec<InstalledPackage>, mgc_types::error::MgError> {
        Err(mgc_types::capabilities::unsupported_capability(
            "adapter",
            "generic installed-package listing",
            "manifest declarations do not prove that packages are materialized; use an ecosystem-specific installed-state verifier",
        ))
    }

    async fn base_update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> Result<Vec<UpdatedPackage>, mgc_types::error::MgError> {
        Err(mgc_types::capabilities::unsupported_capability(
            "adapter",
            "generic update",
            "no ecosystem-specific resolver/update implementation is available",
        ))
    }
}
