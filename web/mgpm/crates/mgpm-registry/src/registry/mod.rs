use std::collections::HashMap;
use std::sync::Arc;

use mgpm_core::{PackageId, PackageName, Version};

pub mod npm;
pub mod jsr;
pub mod git;
pub mod http;
pub mod file;

pub use npm::NpmRegistry;
pub use jsr::JsrRegistry;
pub use git::GitRegistry;
pub use http::HttpRegistry;
pub use file::{FileRegistry, WorkspaceRegistry};

#[derive(Debug, Clone, thiserror::Error)]
pub enum RegistryError {
    #[error("HTTP error: {0}")]
    HttpError(u16),
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("tarball not found")]
    TarballNotFound,
    #[error("not found: {0}")]
    NotFound(String),
}

impl From<reqwest::Error> for RegistryError {
    fn from(e: reqwest::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

impl From<std::io::Error> for RegistryError {
    fn from(e: std::io::Error) -> Self {
        Self::NetworkError(e.to_string())
    }
}

pub struct RegistryManager {
    npm_registries: HashMap<String, Arc<NpmRegistry>>,
}

impl RegistryManager {
    pub fn new() -> Self {
        Self { npm_registries: HashMap::new() }
    }

    pub fn add_npm(&mut self, name: &str, base_url: &str) {
        self.npm_registries.insert(name.to_string(), Arc::new(NpmRegistry::new(base_url)));
    }

    pub fn get_npm(&self, name: &str) -> Option<&Arc<NpmRegistry>> {
        self.npm_registries.get(name)
    }
}

impl Default for RegistryManager {
    fn default() -> Self { Self::new() }
}