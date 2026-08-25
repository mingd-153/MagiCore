//! Fetcher — graph → tải tarballs + import CAS.

use async_trait::async_trait;
use mgc_types::adapter::ResolvedGraph;
use mgc_types::error::MgResult;

#[async_trait]
pub trait Fetcher: Send + Sync {
    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()>;
}
