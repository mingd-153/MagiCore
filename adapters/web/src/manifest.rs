/// package.json manifest handling
use anyhow::Result;
use std::path::Path;

/// Parse package.json
pub fn parse_manifest(path: &Path) -> Result<super::PackageJson> {
    super::PackageJson::load(path)
}

/// Write package.json
pub fn write_manifest(path: &Path, manifest: &super::PackageJson) -> Result<()> {
    manifest.save(path)
}
