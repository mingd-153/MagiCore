//! `manifest/mod.rs` — Manifest parsing for mobile app platforms.
//! Parses pubspec.yaml (Flutter), build.gradle (Kotlin), Package.swift (Swift), package.json (React Native).

pub mod flutter;
pub mod gradle;
pub mod react_native;
pub mod swift;

use crate::language::AppLanguage;
use mgc_types::{Manifest, MgResult};
use std::path::Path;

/// Parse manifest for app project based on language.
/// Parse manifest cho app project theo language.
pub fn parse_manifest(language: AppLanguage, project_root: &Path) -> MgResult<Manifest> {
    match language {
        AppLanguage::Flutter => flutter::parse_pubspec(project_root),
        AppLanguage::Kotlin => gradle::parse_build_gradle(project_root),
        AppLanguage::Swift => swift::parse_package_swift(project_root),
        AppLanguage::ReactNative => react_native::parse_package_json(project_root),
        AppLanguage::ObjC => flutter::parse_podfile(project_root), // ObjC uses CocoaPods
        AppLanguage::Multi => parse_multi_manifest(project_root),
    }
}

/// Parse multi-platform manifest (detect primary platform).
/// Parse manifest multi-platform (detect platform chính).
fn parse_multi_manifest(project_root: &Path) -> MgResult<Manifest> {
    // Try Flutter first
    if project_root.join("pubspec.yaml").exists() {
        return flutter::parse_pubspec(project_root);
    }

    // Try Kotlin (Android)
    if project_root.join("build.gradle").exists() || project_root.join("build.gradle.kts").exists()
    {
        return gradle::parse_build_gradle(project_root);
    }

    // Try Swift (iOS)
    if project_root.join("Package.swift").exists() {
        return swift::parse_package_swift(project_root);
    }

    // Try React Native
    if project_root.join("package.json").exists() {
        return react_native::parse_package_json(project_root);
    }

    // Fallback: empty manifest with project name
    let name = project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".to_string());

    Ok(Manifest::new(&name, mgc_types::Ecosystem::App))
}

/// Write manifest back to file.
/// Viết manifest trả về file.
pub fn write_manifest(
    language: AppLanguage,
    project_root: &Path,
    manifest: &Manifest,
) -> MgResult<()> {
    match language {
        AppLanguage::Flutter => flutter::write_pubspec(project_root, manifest),
        AppLanguage::Kotlin => gradle::write_build_gradle(project_root, manifest),
        AppLanguage::Swift => swift::write_package_swift(project_root, manifest),
        AppLanguage::ReactNative => react_native::write_package_json(project_root, manifest),
        AppLanguage::ObjC => flutter::write_podfile(project_root, manifest),
        AppLanguage::Multi => write_multi_manifest(project_root, manifest),
    }
}

/// Write multi-platform manifest.
/// Viết manifest multi-platform.
fn write_multi_manifest(project_root: &Path, manifest: &Manifest) -> MgResult<()> {
    // Write to primary manifest file found
    if project_root.join("pubspec.yaml").exists() {
        return flutter::write_pubspec(project_root, manifest);
    }

    if project_root.join("build.gradle").exists() || project_root.join("build.gradle.kts").exists()
    {
        return gradle::write_build_gradle(project_root, manifest);
    }

    if project_root.join("Package.swift").exists() {
        return swift::write_package_swift(project_root, manifest);
    }

    if project_root.join("package.json").exists() {
        return react_native::write_package_json(project_root, manifest);
    }

    Ok(())
}
