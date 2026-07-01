pub mod plugins;

pub use plugins::builtin::audit::AuditWarning;
pub use plugins::builtin::dep_graph::{GraphReport, NodeDegrees};
pub use plugins::builtin::license_check::LicenseWarning;
pub use plugins::builtin::size_report::{PackageSize, SizeCategories, SizeReport};
pub use plugins::builtin::{
    AuditPlugin, BuiltinPlugin, BuiltinPluginRegistry, DepGraphPlugin, LicenseCheckPlugin,
    SizeReportPlugin,
};
pub use plugins::{
    create_plugin_host, DepGraph, PackageData, PackageInfo, Plugin, PluginHooks, PluginHost,
    PluginResult, Resolution,
};
