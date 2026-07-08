use async_trait::async_trait;
use std::path::{Path, PathBuf};

use mg_types::adapter::{AddOptions, InstalledPackage, PackageAdapter, UpdatedPackage};
use mg_types::error::MgResult;
use mg_types::package::{DependencySpec, PackageId, PackageName, VersionRange};
use mg_types::version::Version;

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
///   These methods are NOT atomic. Concurrent `mg add` + `mg remove` on the
///   same project will race on read-modify-write. A file-level advisory lock
///   (.megagate/.lock) is planned but not yet implemented.
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
    ) -> Result<(), mg_types::error::MgError> {
        let mut manifest = self.parse_manifest(project_root).await?;
        manifest.remove_dep(name.as_str());
        self.write_manifest(project_root, &manifest).await?;
        Ok(())
    }

    async fn base_list(
        &self,
        project_root: &Path,
    ) -> Result<Vec<InstalledPackage>, mg_types::error::MgError> {
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
    ) -> Result<Vec<UpdatedPackage>, mg_types::error::MgError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_types::adapter::{AuditReport, InstallSummary, ResolvedGraph};
    use mg_types::ecosystem::Ecosystem;
    use mg_types::manifest::Manifest;

    struct TestAdapter;
    #[async_trait]
    impl BaseAdapter for TestAdapter {}

    struct CustomInstallRootAdapter;
    #[async_trait]
    impl BaseAdapter for CustomInstallRootAdapter {
        fn install_root(&self, project_root: &Path) -> PathBuf {
            project_root.join("vendor").join("deps")
        }
    }

    #[async_trait]
    impl PackageAdapter for TestAdapter {
        fn name(&self) -> &str {
            "test"
        }
        fn ecosystem(&self) -> Ecosystem {
            Ecosystem::Web
        }
        fn can_handle(&self, _: &Path) -> bool {
            true
        }
        async fn parse_manifest(&self, _: &Path) -> MgResult<Manifest> {
            Ok(Manifest::new("test", Ecosystem::Web))
        }
        async fn write_manifest(&self, _: &Path, _: &Manifest) -> MgResult<()> {
            Ok(())
        }
        async fn resolve(&self, _: &Manifest) -> MgResult<ResolvedGraph> {
            Ok(ResolvedGraph::empty())
        }
        async fn fetch(&self, _: &ResolvedGraph) -> MgResult<()> {
            Ok(())
        }
        async fn install(&self, _: &ResolvedGraph, _: &Path) -> MgResult<InstallSummary> {
            Ok(InstallSummary::default())
        }
        async fn add(
            &self,
            _: &Path,
            _: &PackageName,
            _: Option<&VersionRange>,
            _: AddOptions,
        ) -> MgResult<PackageId> {
            Ok(PackageId::new(
                PackageName::new("test").unwrap(),
                Version::new(0, 0, 0),
            ))
        }
        async fn remove(&self, _: &Path, _: &PackageName) -> MgResult<()> {
            Ok(())
        }
        async fn update(&self, _: &Path, _: Option<&PackageName>) -> MgResult<Vec<UpdatedPackage>> {
            Ok(vec![])
        }
        async fn list(&self, _: &Path) -> MgResult<Vec<InstalledPackage>> {
            Ok(vec![])
        }
        async fn audit(&self, _: &Path) -> MgResult<AuditReport> {
            Ok(AuditReport::clean(0))
        }
    }

    #[async_trait]
    impl PackageAdapter for CustomInstallRootAdapter {
        fn name(&self) -> &str {
            "custom"
        }
        fn ecosystem(&self) -> Ecosystem {
            Ecosystem::Lib
        }
        fn can_handle(&self, _: &Path) -> bool {
            true
        }
        async fn parse_manifest(&self, _: &Path) -> MgResult<Manifest> {
            let mut manifest = Manifest::new("custom", Ecosystem::Lib);
            manifest.add_dep(
                DependencySpec::new(
                    PackageName::new("serde").unwrap(),
                    VersionRange::parse("^1.0.0").unwrap(),
                ),
                false,
                false,
                false,
            );
            Ok(manifest)
        }
        async fn write_manifest(&self, _: &Path, _: &Manifest) -> MgResult<()> {
            Ok(())
        }
        async fn resolve(&self, _: &Manifest) -> MgResult<ResolvedGraph> {
            Ok(ResolvedGraph::empty())
        }
        async fn fetch(&self, _: &ResolvedGraph) -> MgResult<()> {
            Ok(())
        }
        async fn install(&self, _: &ResolvedGraph, _: &Path) -> MgResult<InstallSummary> {
            Ok(InstallSummary::default())
        }
        async fn add(
            &self,
            _: &Path,
            _: &PackageName,
            _: Option<&VersionRange>,
            _: AddOptions,
        ) -> MgResult<PackageId> {
            Ok(PackageId::new(
                PackageName::new("serde").unwrap(),
                Version::new(1, 0, 0),
            ))
        }
        async fn remove(&self, _: &Path, _: &PackageName) -> MgResult<()> {
            Ok(())
        }
        async fn update(&self, _: &Path, _: Option<&PackageName>) -> MgResult<Vec<UpdatedPackage>> {
            Ok(vec![])
        }
        async fn list(&self, _: &Path) -> MgResult<Vec<InstalledPackage>> {
            Ok(vec![])
        }
        async fn audit(&self, _: &Path) -> MgResult<AuditReport> {
            Ok(AuditReport::clean(0))
        }
    }

    #[test]
    fn test_normalize_range_exact() {
        let r = VersionRange::parse("^1.2.3").unwrap();
        let norm = <TestAdapter as BaseAdapter>::normalize_range(Some(&r), true);
        assert_eq!(norm.unwrap().as_str(), "1.2.3");
    }

    #[tokio::test]
    async fn test_base_list_uses_adapter_install_root() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = CustomInstallRootAdapter;

        let packages = adapter.base_list(dir.path()).await.unwrap();

        assert_eq!(packages.len(), 1);
        assert_eq!(
            packages[0].path,
            dir.path().join("vendor").join("deps").join("serde")
        );
    }
}
