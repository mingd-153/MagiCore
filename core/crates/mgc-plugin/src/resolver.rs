//! Resolver — manifest → ResolvedGraph.

use async_trait::async_trait;
use mgc_types::adapter::ResolvedGraph;
use mgc_types::error::MgResult;
use mgc_types::manifest::Manifest;

#[async_trait]
pub trait Resolver: Send + Sync {
    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph>;
}
