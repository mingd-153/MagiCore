//! Fetcher — graph → tải tarballs + import CAS.

use async_trait::async_trait;
use mg_types::adapter::ResolvedGraph;
use mg_types::error::MgResult;

#[async_trait]
pub trait Fetcher: Send + Sync {
    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()>;
}
