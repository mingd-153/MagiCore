//! React Native package.json manifest parsing (delegates to web adapter logic).

use mgc_types::{Ecosystem, Manifest, MgError, MgResult};
use std::path::Path;

/// Parse package.json to Manifest (React Native).
pub fn parse_package_json(project_root: &Path) -> MgResult<Manifest> {
    let package_json_path = project_root.join("package.json");

    if !package_json_path.exists() {
        return Err(MgError::Other("package.json not found".to_string()));
    }

    let content = std::fs::read_to_string(&package_json_path)
        .map_err(|e| MgError::Other(format!("failed to read package.json: {}", e)))?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| MgError::Other(format!("failed to parse package.json: {}", e)))?;

    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("app")
        .to_string();

    // TODO: Parse dependencies (can delegate to web adapter manifest parser)
    Ok(Manifest::new(&name, Ecosystem::App))
}

/// Write Manifest back to package.json.
pub fn write_package_json(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    // TODO: Implement package.json write (delegate to web adapter)
    Ok(())
}
