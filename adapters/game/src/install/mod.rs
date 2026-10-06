//! Public Game install facade. Package installation is unsupported until
//! MagiCore owns the selected engine's resolver, lock, fetch, and materializer.
//! Facade install Game công khai; hiện fail-closed tới khi MGC sở hữu đủ engine.

use crate::engine::GameEngine;
use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Install dependencies for a game project.
/// Install dependency cho project game.
pub async fn install_dependencies(
    engine: GameEngine,
    _project_root: &Path,
) -> MgResult<InstallSummary> {
    Err(MgError::Unsupported {
        core: "game",
        capability: "install",
        guidance: format!(
            "MagiCore does not own dependency installation for the {engine:?} engine; no provider package manager was invoked"
        ),
    })
}

/// Summary retained for source compatibility; unsupported installs never
/// fabricate one.
/// Giữ kiểu summary để tương thích source; install unsupported không bịa summary.
#[derive(Debug, Clone)]
pub struct InstallSummary {
    pub engine: GameEngine,
    pub installed_packages: Vec<String>,
    pub bytes_downloaded: u64,
    pub duration_ms: u64,
    pub verified: bool,
}

#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
