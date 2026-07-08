/// User-level configuration
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    pub name: Option<String>,
    pub email: Option<String>,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            name: None,
            email: None,
        }
    }
}
