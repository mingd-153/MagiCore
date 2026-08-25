//! Linker — graph → materialize node_modules/vendor tree.

use async_trait::async_trait;
use mgc_types::adapter::{InstallOptions, InstallSummary, ResolvedGraph};
use mgc_types::error::MgResult;
use std::path::Path;

#[async_trait]
pub trait Linker: Send + Sync {
    async fn link(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary>;
}
