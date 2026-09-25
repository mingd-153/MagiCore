//! React Native package.json manifest parsing through MagiCore's web manifest engine.

use mgc_types::{Manifest, MgResult};
use std::path::Path;

/// Parse package.json using the same native parser as the Web JS engine.
pub fn parse_package_json(project_root: &Path) -> MgResult<Manifest> {
    mgc_web_adapter::manifest::parse_manifest(project_root)
}

/// Write package.json using MagiCore's shared native Web JS manifest writer.
pub fn write_package_json(project_root: &Path, manifest: &Manifest) -> MgResult<()> {
    mgc_web_adapter::manifest::write_manifest(project_root, manifest)
}
