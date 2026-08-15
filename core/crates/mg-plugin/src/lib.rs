#![forbid(unsafe_code)]

//! Plugin architecture — tách pipeline thành 5 trait (T3).
//!
//! Back-compat: `PackageAdapter` monolith (mg-types) giữ nguyên; adapter cũ
//! map lên qua `AsPlugin::as_plugin()` — 0 hành vi đổi. Plugin mới đi 5 trait
//! trực tiếp.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use mg_types::adapter::{
    InstallOptions, InstallSummary, PackageAdapter, ResolvedGraph,
};
use mg_types::error::MgResult;
use mg_types::manifest::Manifest;
use mg_types::Ecosystem;

pub mod backend;
pub mod fetcher;
pub mod linker;
pub mod resolver;
pub mod template;

pub use backend::CacheBackend;
pub use fetcher::Fetcher;
pub use linker::Linker;
pub use resolver::Resolver;
pub use template::TemplateGenerator;

/// Plugin — 1 ecosystem ghép từ 5 trait component (dùng default khi không override).
#[derive(Clone)]
pub struct Plugin {
    pub ecosystem: Ecosystem,
    pub resolver: Arc<dyn Resolver>,
    pub fetcher: Arc<dyn Fetcher>,
    pub linker: Arc<dyn Linker>,
    pub template: Arc<dyn TemplateGenerator>,
    pub cache: Arc<dyn CacheBackend>,
    adapter: Option<Arc<dyn PackageAdapter>>,
}

impl Plugin {
    /// Plugin từ adapter monolith cũ — mỗi trait gọi method tương ứng (back-compat).
    pub fn from_adapter(adapter: Arc<dyn PackageAdapter>) -> Self {
        let resolver = Arc::new(adapter::ResolverBridge(adapter.clone()));
        let fetcher = Arc::new(adapter::FetcherBridge(adapter.clone()));
        let linker = Arc::new(adapter::LinkerBridge(adapter.clone()));
        Self {
            ecosystem: adapter.ecosystem(),
            resolver,
            fetcher,
            linker,
            template: Arc::new(adapter::TemplateUnsupported {
                ecosystem: adapter.ecosystem(),
            }),
            cache: Arc::new(adapter::CacheUnsupported {
                ecosystem: adapter.ecosystem(),
            }),
            adapter: Some(adapter),
        }
    }

    /// Registry lookup miss → thử adapter backend map từ PackageAdapter (không đăng ký lại).
    pub fn ecosystem(&self) -> Ecosystem {
        self.ecosystem
    }

    /// Back-ref tới PackageAdapter gốc (plugin tạo từ monolith). None = plugin thuần 5 trait.
    pub fn as_adapter(&self) -> Option<Arc<dyn PackageAdapter>> {
        self.adapter.clone()
    }
}

mod adapter {
    use super::*;

    pub struct ResolverBridge(pub Arc<dyn PackageAdapter>);
    #[async_trait]
    impl Resolver for ResolverBridge {
        async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
            self.0.resolve(manifest).await
        }
    }

    pub struct FetcherBridge(pub Arc<dyn PackageAdapter>);
    #[async_trait]
    impl Fetcher for FetcherBridge {
        async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
            self.0.fetch(graph).await
        }
    }

    pub struct LinkerBridge(pub Arc<dyn PackageAdapter>);
    #[async_trait]
    impl Linker for LinkerBridge {
        async fn link(
            &self,
            graph: &ResolvedGraph,
            project_root: &Path,
            opts: InstallOptions,
        ) -> MgResult<InstallSummary> {
            self.0.install(graph, project_root, opts).await
        }
    }

    pub struct TemplateUnsupported {
        pub ecosystem: Ecosystem,
    }
    #[async_trait]
    impl TemplateGenerator for TemplateUnsupported {
        async fn generate(&self, _: &Path, _: &str) -> MgResult<Manifest> {
            Err(mg_types::error::MgError::Other(format!(
                "template generator not implemented for '{}'",
                self.ecosystem.as_str()
            )))
        }
    }

    pub struct CacheUnsupported {
        pub ecosystem: Ecosystem,
    }
    #[async_trait]
    impl CacheBackend for CacheUnsupported {
        async fn get(&self, _: &str) -> MgResult<Option<std::path::PathBuf>> {
            Err(mg_types::error::MgError::Other(format!(
                "cache backend not implemented for '{}'",
                self.ecosystem.as_str()
            )))
        }
        async fn put(&self, _: &str, _: &[u8]) -> MgResult<()> {
            Err(mg_types::error::MgError::Other(format!(
                "cache backend not implemented for '{}'",
                self.ecosystem.as_str()
            )))
        }
        async fn claim(&self, _: &std::path::Path, _: &[String]) -> MgResult<()> {
            Err(mg_types::error::MgError::Other(format!(
                "cache backend not implemented for '{}'",
                self.ecosystem.as_str()
            )))
        }
        async fn release(&self, _: &std::path::Path) -> MgResult<()> {
            Err(mg_types::error::MgError::Other(format!(
                "cache backend not implemented for '{}'",
                self.ecosystem.as_str()
            )))
        }
    }
}

/// PluginRegistry — bảng `ecosystem → Plugin` (tĩnh, tự viết style Yarn-inspired).
#[derive(Default)]
pub struct PluginRegistry {
    by_ecosystem: Mutex<HashMap<&'static str, Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, plugin: Plugin) -> Result<(), String> {
        let key = plugin.ecosystem.as_str();
        let mut map = self.by_ecosystem.lock().unwrap();
        if map.contains_key(key) {
            return Err(format!(
                "plugin already registered for ecosystem '{}'",
                key
            ));
        }
        map.insert(key, plugin);
        Ok(())
    }

    pub fn get(&self, ecosystem: Ecosystem) -> Option<Plugin> {
        self.by_ecosystem
            .lock()
            .unwrap()
            .get(ecosystem.as_str())
            .cloned()
    }

    pub fn registered(&self) -> Vec<Ecosystem> {
        self.by_ecosystem
            .lock()
            .unwrap()
            .keys()
            .map(|key| Ecosystem::from_str(key).unwrap())
            .collect()
    }
}

fn global_registry() -> &'static PluginRegistry {
    static REGISTRY: OnceLock<PluginRegistry> = OnceLock::new();
    REGISTRY.get_or_init(PluginRegistry::new)
}

/// Global registry — cli dispatch hỏi global thay vì match cứng.
pub fn global() -> &'static PluginRegistry {
    global_registry()
}

/// Đăng ký plugin toàn cục; trả Err nếu ecosystem đã có (không tự ghi đè).
pub fn register(plugin: Plugin) -> Result<(), String> {
    global().register(plugin)
}