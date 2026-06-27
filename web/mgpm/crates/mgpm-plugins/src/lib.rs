pub mod plugins;

pub use plugins::{
    Plugin, PluginHost, PluginHooks, PluginResult, Resolution, PackageData, PackageInfo,
    DepGraph, create_plugin_host,
};
pub use plugins::builtin::{
    AuditPlugin, BuiltinPlugin, BuiltinPluginRegistry, DepGraphPlugin, LicenseCheckPlugin,
    SizeReportPlugin,
};
pub use plugins::builtin::audit::AuditWarning;
pub use plugins::builtin::dep_graph::{GraphReport, NodeDegrees};
pub use plugins::builtin::license_check::LicenseWarning;
pub use plugins::builtin::size_report::{PackageSize, SizeCategories, SizeReport};
