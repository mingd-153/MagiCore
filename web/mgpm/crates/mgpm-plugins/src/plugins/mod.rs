//! Plugin System with napi-rs - Rollup-compatible hooks

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use napi::{bindgen_prelude::*, Result};
use napi_derive::napi;

#[napi]
pub struct PluginHost {
    plugins: Arc<RwLock<HashMap<String, Plugin>>>,
}

#[napi]
impl PluginHost {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[napi]
    pub async fn load(&self, name: String, path: String) -> Result<()> {
        let plugin = Plugin {
            name: name.clone(),
            path,
            hooks: PluginHooks::default(),
        };
        let mut plugins = self.plugins.write().await;
        plugins.insert(name, plugin);
        Ok(())
    }

    #[napi]
    pub async fn get(&self, name: String) -> Result<Option<Plugin>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.get(&name).cloned())
    }

    #[napi]
    pub async fn list(&self) -> Result<Vec<String>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.keys().cloned().collect())
    }
}

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct PluginHooks {
    pub resolve_spec: Option<String>,
    pub fetch_package: Option<String>,
    pub pre_install: Option<String>,
    pub post_install: Option<String>,
    pub pre_link: Option<String>,
    pub post_link: Option<String>,
    pub pre_script: Option<String>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub path: String,
    pub hooks: PluginHooks,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct Resolution {
    pub package: String,
    pub version: String,
    pub tarball: String,
    pub integrity: String,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct PackageData {
    pub name: String,
    pub version: String,
    pub files: Vec<String>,
    pub tarball: String,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct DepGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

#[napi]
pub async fn create_plugin_host() -> Result<PluginHost> {
    Ok(PluginHost::new())
}