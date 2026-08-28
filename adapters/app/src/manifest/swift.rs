//! Swift Package.swift manifest parsing.

use mgc_types::{Ecosystem, Manifest, MgError, MgResult};
use std::path::Path;

/// Parse Package.swift to Manifest.
pub fn parse_package_swift(project_root: &Path) -> MgResult<Manifest> {
    let swift_path = project_root.join("Package.swift");

    if !swift_path.exists() {
        return Err(MgError::Other("Package.swift not found".to_string()));
    }

    // TODO: Parse Package.swift (Swift code parsing)
    // For now, return empty manifest with project name
    let name = project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".to_string());

    Ok(Manifest::new(&name, Ecosystem::App))
}

/// Write Manifest back to Package.swift.
pub fn write_package_swift(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    // TODO: Implement Package.swift write
    Ok(())
}
