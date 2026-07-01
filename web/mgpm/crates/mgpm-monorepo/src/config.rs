use mgpm_core::ScriptConfig;
use std::collections::HashMap;

pub struct MonorepoConfig {
    pub scripts: HashMap<String, ScriptConfig>,
}

impl MonorepoConfig {
    pub fn new(scripts: HashMap<String, ScriptConfig>) -> Self {
        Self { scripts }
    }
}
