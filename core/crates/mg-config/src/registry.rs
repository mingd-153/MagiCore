/// Registry configuration
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub name: String,
    pub url: String,
    pub priority: u32,
}

impl Registry {
    pub fn new(name: String, url: String) -> Self {
        Self {
            name,
            url,
            priority: 0,
        }
    }
}
