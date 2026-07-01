pub mod audit;
pub mod dep_graph;
pub mod license_check;
pub mod size_report;

use super::PluginResult;

pub use audit::AuditPlugin;
pub use dep_graph::DepGraphPlugin;
pub use license_check::LicenseCheckPlugin;
pub use size_report::SizeReportPlugin;

/// Trait for built-in plugins that can be registered with the PluginHost.
/// These run in-process (not as external JS plugins) and provide core
/// functionality like auditing, license checking, size reporting, and
/// dependency graph analysis.
pub trait BuiltinPlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, hook: &str, data: &serde_json::Value) -> PluginResult;
}

/// Registry for built-in plugins. Manages lifecycle and hook dispatch
/// for all in-process plugin instances.
pub struct BuiltinPluginRegistry {
    plugins: Vec<Box<dyn BuiltinPlugin>>,
}

impl BuiltinPluginRegistry {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }
}

impl Default for BuiltinPluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinPluginRegistry {
    pub fn register<P: BuiltinPlugin + 'static>(&mut self, plugin: P) {
        self.plugins.push(Box::new(plugin));
    }

    pub fn register_all(&mut self) {
        self.register(AuditPlugin);
        self.register(LicenseCheckPlugin::new(vec![]));
        self.register(SizeReportPlugin);
        self.register(DepGraphPlugin);
    }

    pub fn get(&self, name: &str) -> Option<&dyn BuiltinPlugin> {
        self.plugins.iter().find(|p| p.name() == name).map(|p| p.as_ref())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let len_before = self.plugins.len();
        self.plugins.retain(|p| p.name() != name);
        self.plugins.len() < len_before
    }

    pub fn list(&self) -> Vec<String> {
        self.plugins.iter().map(|p| p.name().to_string()).collect()
    }

    pub fn run_hook(&self, hook: &str, data: &serde_json::Value) -> Vec<PluginResult> {
        self.plugins.iter().map(|p| p.run(hook, data)).collect()
    }
}
