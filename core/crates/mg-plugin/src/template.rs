//! TemplateGenerator — scaffold manifest mới cho ecosystem.

use async_trait::async_trait;
use mg_types::error::MgResult;
use mg_types::manifest::Manifest;
use std::path::Path;

#[async_trait]
pub trait TemplateGenerator: Send + Sync {
    async fn generate(&self, project_root: &Path, name: &str) -> MgResult<Manifest>;
}
