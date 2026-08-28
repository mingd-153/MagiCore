//! `install/fetch.rs` — Package fetching for mobile app platforms.
//! URL builders for pub.dev (Flutter), Maven (Kotlin), CocoaPods (iOS).

use mgc_types::{MgError, MgResult, PackageId};
use std::path::PathBuf;

/// Fetch Flutter package from pub.dev.
/// Tải Flutter package từ pub.dev.
pub fn fetch_flutter_package(
    _project_root: &std::path::Path,
    _package_id: &PackageId,
) -> MgResult<PathBuf> {
    // Flutter packages are cached in ~/.pub-cache
    let cache_dir = dirs::home_dir()
        .ok_or_else(|| MgError::Other("cannot find home directory".to_string()))?
        .join(".pub-cache/hosted/pub.dev");

    let package_dir = cache_dir.join(format!("{}-{}", _package_id.name(), _package_id.version()));

    if package_dir.exists() {
        Ok(package_dir)
    } else {
        Err(MgError::Other(format!(
            "Flutter package not found in cache: {}",
            package_dir.display()
        )))
    }
}

/// Fetch Kotlin/Android package from Maven Central.
/// Tải Kotlin/Android package từ Maven Central.
pub fn fetch_kotlin_package(
    _project_root: &std::path::Path,
    _package_id: &PackageId,
) -> MgResult<PathBuf> {
    // Gradle caches in ~/.gradle/caches/modules-2/files-2.1/
    let cache_dir = dirs::home_dir()
        .ok_or_else(|| MgError::Other("cannot find home directory".to_string()))?
        .join(".gradle/caches/modules-2/files-2.1");

    // Maven coordinates: group:artifact:version
    // Cache structure: group/artifact/version/hash/artifact-version.jar
    let package_dir = cache_dir.join(_package_id.name().as_str());

    if package_dir.exists() {
        Ok(package_dir)
    } else {
        Err(MgError::Other(format!(
            "Gradle package not found in cache: {}",
            package_dir.display()
        )))
    }
}

/// Fetch Swift package from Swift Package Manager cache.
/// Tải Swift package từ Swift Package Manager cache.
pub fn fetch_swift_package(
    _project_root: &std::path::Path,
    _package_id: &PackageId,
) -> MgResult<PathBuf> {
    // SPM caches in ~/Library/Caches/org.swift.swiftpm/
    let cache_dir = if cfg!(target_os = "macos") {
        dirs::home_dir()
            .ok_or_else(|| MgError::Other("cannot find home directory".to_string()))?
            .join("Library/Caches/org.swift.swiftpm")
    } else {
        dirs::cache_dir()
            .ok_or_else(|| MgError::Other("cannot find cache directory".to_string()))?
            .join("org.swift.swiftpm")
    };

    let package_dir = cache_dir
        .join("repositories")
        .join(_package_id.name().as_str());

    if package_dir.exists() {
        Ok(package_dir)
    } else {
        Err(MgError::Other(format!(
            "Swift package not found in cache: {}",
            package_dir.display()
        )))
    }
}

/// Fetch CocoaPods pod from Pods/ directory.
/// Tải CocoaPods pod từ thư mục Pods/.
pub fn fetch_cocoapods_pod(
    project_root: &std::path::Path,
    _package_id: &PackageId,
) -> MgResult<PathBuf> {
    let pods_dir = project_root.join("Pods").join(_package_id.name().as_str());

    if pods_dir.exists() {
        Ok(pods_dir)
    } else {
        Err(MgError::Other(format!(
            "CocoaPods pod not found: {}",
            pods_dir.display()
        )))
    }
}

/// Construct URL for Flutter package on pub.dev.
/// Xây dựng URL cho Flutter package trên pub.dev.
pub fn pub_dev_package_url(package_name: &str) -> String {
    format!("https://pub.dev/packages/{}", package_name)
}

/// Construct URL for Maven artifact.
/// Xây dựng URL cho Maven artifact.
pub fn maven_central_url(group_id: &str, artifact_id: &str, version: &str) -> String {
    let group_path = group_id.replace('.', "/");
    format!(
        "https://repo1.maven.org/maven2/{}/{}/{}/{}-{}.jar",
        group_path, artifact_id, version, artifact_id, version
    )
}

/// Construct URL for CocoaPods spec.
/// Xây dựng URL cho CocoaPods spec.
pub fn cocoapods_spec_url(pod_name: &str) -> String {
    format!("https://cdn.cocoapods.org/Specs/{}.podspec.json", pod_name)
}
