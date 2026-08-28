//! Gradle build.gradle / build.gradle.kts manifest parsing.

use mgc_types::{Ecosystem, Manifest, MgError, MgResult};
use std::path::Path;

/// Parse build.gradle or build.gradle.kts to Manifest.
pub fn parse_build_gradle(project_root: &Path) -> MgResult<Manifest> {
    let gradle_path = if project_root.join("build.gradle.kts").exists() {
        project_root.join("build.gradle.kts")
    } else {
        project_root.join("build.gradle")
    };

    if !gradle_path.exists() {
        return Err(MgError::Other("build.gradle not found".to_string()));
    }

    // TODO: Parse Gradle DSL (Groovy/Kotlin)
    // For now, return empty manifest with project name
    let name = project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".to_string());

    Ok(Manifest::new(&name, Ecosystem::App))
}

/// Write Manifest back to build.gradle.
pub fn write_build_gradle(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    // TODO: Implement Gradle DSL write
    Ok(())
}
