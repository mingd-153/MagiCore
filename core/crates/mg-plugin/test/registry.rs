#![allow(clippy::unwrap_used)]
//! PluginRegistry tests — RULE §5 (test/ ngoài src/).

use mg_plugin::Plugin;
use mg_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    ResolvedGraph, UpdatedPackage,
};
use mg_types::error::MgResult;
use mg_types::manifest::Manifest;
use mg_types::package::{PackageId, PackageName, VersionRange};
use mg_types::version::Version;
use mg_types::Ecosystem;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

async fn spawn_adapter_plugin() -> Arc<dyn PackageAdapter> {
    struct DummyAdapter;
    #[async_trait]
    impl PackageAdapter for DummyAdapter {
        fn name(&self) -> &str {
            "dummy"
        }
        fn ecosystem(&self) -> Ecosystem {
            Ecosystem::Web
        }
        fn can_handle(&self, _: &Path) -> bool {
            true
        }
        async fn parse_manifest(&self, _: &Path) -> MgResult<Manifest> {
            Ok(Manifest::new("dummy", Ecosystem::Web))
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
        async fn install(
            &self,
            _: &ResolvedGraph,
            _: &Path,
            _: InstallOptions,
        ) -> MgResult<InstallSummary> {
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
                PackageName::new("dummy").unwrap(),
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
    Arc::new(DummyAdapter)
}

fn stub_plugin() -> Plugin {
    // Components mặc định (resolver/fetcher/linker bridge) qua adapter dummy —
    // Plugin fields private nên test dùng đường công khai from_adapter.
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let adapter = spawn_adapter_plugin().await;
        Plugin::from_adapter(adapter)
    })
}

#[test]
fn registry_get_returns_registered_plugin() {
    let registry = mg_plugin::PluginRegistry::new();
    registry.register(stub_plugin()).unwrap();
    let plugin = registry.get(Ecosystem::Web).expect("web plugin");
    assert_eq!(plugin.ecosystem(), Ecosystem::Web);
    assert_eq!(registry.registered(), vec![Ecosystem::Web]);
}

#[test]
fn registry_duplicate_register_rejected() {
    let registry = mg_plugin::PluginRegistry::new();
    registry.register(stub_plugin()).unwrap();
    let err = registry.register(stub_plugin()).unwrap_err();
    assert!(err.contains("already registered"));
}

#[test]
fn registry_miss_returns_none() {
    let registry = mg_plugin::PluginRegistry::new();
    assert!(registry.get(Ecosystem::Game).is_none());
}

#[tokio::test]
async fn plugin_from_adapter_bridges_resolver_fetcher_linker() {
    let adapter = spawn_adapter_plugin().await;
    let plugin = Plugin::from_adapter(adapter);
    assert_eq!(plugin.ecosystem(), Ecosystem::Web);
    let manifest = Manifest::new("dummy", Ecosystem::Web);
    let graph = plugin.resolver.resolve(&manifest).await.unwrap();
    plugin.fetcher.fetch(&graph).await.unwrap();
    let summary = plugin
        .linker
        .link(&graph, Path::new("/tmp"), InstallOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.bytes_from_cache, 0);
    let err = plugin
        .template
        .generate(Path::new("/tmp"), "x")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not implemented"));
    let err = plugin.cache.get("k").await.unwrap_err();
    assert!(err.to_string().contains("not implemented"));
}

#[tokio::test]
async fn plugin_from_adapter_back_ref_adapter() {
    let adapter = spawn_adapter_plugin().await;
    let plugin = Plugin::from_adapter(adapter.clone());
    let back = plugin.as_adapter().expect("back-ref adapter");
    assert_eq!(back.name(), "dummy");
}

#[test]
fn global_register_and_get() {
    mg_plugin::register(stub_plugin()).expect("first register");
    let plugin = mg_plugin::global().get(Ecosystem::Web).expect("web plugin");
    assert_eq!(plugin.ecosystem(), Ecosystem::Web);
}
