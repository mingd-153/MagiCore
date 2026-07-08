//! Plugin System with napi-rs - Rollup-compatible hooks
//!
//! Supports both external JS plugins (via napi-rs) and built-in Rust plugins.

pub mod builtin;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use tokio::sync::RwLock;

use napi::Result;
use napi_derive::napi;

use builtin::{BuiltinPlugin, BuiltinPluginRegistry};

static BUILTINS: Lazy<Mutex<BuiltinPluginRegistry>> = Lazy::new(|| {
    let mut registry = BuiltinPluginRegistry::new();
    registry.register_all();
    Mutex::new(registry)
});

#[napi(object)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginResult {
    pub success: bool,
    pub message: String,
    pub data: Option<String>,
}

impl PluginResult {
    pub fn pass() -> Self {
        Self {
            success: true,
            message: String::new(),
            data: None,
        }
    }
}

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
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl PluginHost {
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
        let external = plugins.keys().cloned().collect::<Vec<_>>();
        let builtins = BUILTINS.lock().unwrap().list();
        let mut all = external;
        all.extend(builtins);
        Ok(all)
    }

    /// Register all 4 built-in plugins (audit, license-check, size-report, dep-graph).
    #[napi]
    pub async fn register_builtins(&self) {
        let mut builtins = BUILTINS.lock().unwrap();
        builtins.register_all();
    }

    /// Run a hook across all registered built-in plugins.
    /// `hook` is the hook name and `data` is a JSON string passed to each plugin.
    #[napi]
    pub async fn run_hook(&self, hook: String, data: String) -> Vec<PluginResult> {
        let parsed: serde_json::Value =
            serde_json::from_str(&data).unwrap_or(serde_json::Value::Null);
        let builtins = BUILTINS.lock().unwrap();
        builtins.run_hook(&hook, &parsed)
    }

    /// Remove a built-in plugin by name.
    #[napi]
    pub async fn remove_builtin(&self, name: String) -> bool {
        let mut builtins = BUILTINS.lock().unwrap();
        builtins.remove(&name)
    }

    /// List only built-in plugin names.
    #[napi]
    pub async fn list_builtins(&self) -> Vec<String> {
        let builtins = BUILTINS.lock().unwrap();
        builtins.list()
    }
}

#[napi(object)]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Plugin {
    pub name: String,
    pub path: String,
    pub hooks: PluginHooks,
}

#[napi(object)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Resolution {
    pub package: String,
    pub version: String,
    pub tarball: String,
    pub integrity: String,
}

#[napi(object)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageData {
    pub name: String,
    pub version: String,
    pub files: Vec<String>,
    pub tarball: String,
}

#[napi(object)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
    pub integrity: Option<String>,
    pub size: Option<i64>,
    pub license: Option<String>,
}

#[napi(object)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

#[napi]
pub async fn create_plugin_host() -> Result<PluginHost> {
    Ok(PluginHost::new())
}

// --- BuiltinPlugin trait implementations ---

impl BuiltinPlugin for builtin::AuditPlugin {
    fn name(&self) -> &'static str {
        "builtin:audit"
    }

    fn run(&self, hook: &str, data: &serde_json::Value) -> PluginResult {
        match hook {
            "post_install" | "post_link" => {
                let packages: Vec<PackageInfo> =
                    serde_json::from_value(data.clone()).unwrap_or_default();
                builtin::AuditPlugin::run_audit(&packages)
            }
            _ => PluginResult::pass(),
        }
    }
}

impl BuiltinPlugin for builtin::LicenseCheckPlugin {
    fn name(&self) -> &'static str {
        "builtin:license-check"
    }

    fn run(&self, hook: &str, data: &serde_json::Value) -> PluginResult {
        match hook {
            "post_link" => {
                let packages: Vec<PackageInfo> =
                    serde_json::from_value(data.clone()).unwrap_or_default();
                self.check_licenses(&packages)
            }
            _ => PluginResult::pass(),
        }
    }
}

impl BuiltinPlugin for builtin::SizeReportPlugin {
    fn name(&self) -> &'static str {
        "builtin:size-report"
    }

    fn run(&self, hook: &str, data: &serde_json::Value) -> PluginResult {
        match hook {
            "post_link" => {
                let packages: Vec<PackageInfo> =
                    serde_json::from_value(data.clone()).unwrap_or_default();
                builtin::SizeReportPlugin::analyze_sizes(&packages)
            }
            _ => PluginResult::pass(),
        }
    }
}

impl BuiltinPlugin for builtin::DepGraphPlugin {
    fn name(&self) -> &'static str {
        "builtin:dep-graph"
    }

    fn run(&self, hook: &str, data: &serde_json::Value) -> PluginResult {
        match hook {
            "post_link" => {
                let graph: DepGraph =
                    serde_json::from_value(data.clone()).unwrap_or_else(|_| DepGraph {
                        nodes: vec![],
                        edges: vec![],
                    });
                builtin::DepGraphPlugin::generate_graph(&graph)
            }
            _ => PluginResult::pass(),
        }
    }
}
