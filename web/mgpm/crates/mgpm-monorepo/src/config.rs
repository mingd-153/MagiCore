use mgpm_core::ScriptConfig;
use std::collections::HashMap;

/// Monorepo-specific configuration wrapper.
///
/// Holds the parsed `ScriptConfig` entries from `mgpm.yaml` for use by
/// [`TaskGraph`](crate::TaskGraph) and [`TaskExecutor`](crate::TaskExecutor).
pub struct MonorepoConfig {
    /// Map of script name → configuration.
    pub scripts: HashMap<String, ScriptConfig>,
}

impl MonorepoConfig {
    pub fn new(scripts: HashMap<String, ScriptConfig>) -> Self {
        Self { scripts }
    }
}
