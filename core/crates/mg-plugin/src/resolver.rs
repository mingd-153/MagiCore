//! Resolver — manifest → ResolvedGraph.

use async_trait::async_trait;
use mg_types::adapter::ResolvedGraph;
use mg_types::error::MgResult;
use mg_types::manifest::Manifest;

#[async_trait]
pub trait Resolver: Send + Sync {
    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph>;
}