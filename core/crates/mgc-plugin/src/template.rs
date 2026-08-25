//! TemplateGenerator — scaffold manifest mới cho ecosystem.

use async_trait::async_trait;
use mgc_types::error::MgResult;
use mgc_types::manifest::Manifest;
use std::path::Path;

#[async_trait]
pub trait TemplateGenerator: Send + Sync {
    async fn generate(&self, project_root: &Path, name: &str) -> MgResult<Manifest>;
}
