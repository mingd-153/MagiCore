#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Install module tests for app adapter.

use mgc_app_adapter::install::fetch::*;
use mgc_app_adapter::install::verify::*;

#[test]
fn pub_dev_package_url_format() {
    let url = pub_dev_package_url("http");
    assert_eq!(url, "https://pub.dev/packages/http");
}

#[test]
fn maven_central_url_format() {
    let url = maven_central_url("com.google.android", "material", "1.0.0");
    assert_eq!(
        url,
        "https://repo1.maven.org/maven2/com/google/android/material/1.0.0/material-1.0.0.jar"
    );
}

#[test]
fn cocoapods_spec_url_format() {
    let url = cocoapods_spec_url("Alamofire");
    assert_eq!(
        url,
        "https://cdn.cocoapods.org/Specs/Alamofire.podspec.json"
    );
}

#[test]
fn verify_pubspec_lock_missing_file() {
    let tmp = std::env::temp_dir().join(format!("mgc-app-verify-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let result = verify_pubspec_lock(&tmp);
    assert!(result.is_err());

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn verify_gradle_lockfile_optional() {
    let tmp = std::env::temp_dir().join(format!("mgc-app-gradle-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    // gradle.lockfile is optional - should not error
    let result = verify_gradle_lockfile(&tmp);
    assert!(result.is_ok());

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn verify_package_resolved_missing_file() {
    let tmp = std::env::temp_dir().join(format!("mgc-app-swift-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let result = verify_package_resolved(&tmp);
    assert!(result.is_err());

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn verify_package_file_with_hash() {
    let tmp = std::env::temp_dir().join(format!("mgc-app-hash-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let file_path = tmp.join("test.tar.gz");
    let content = b"test flutter package";
    std::fs::write(&file_path, content).unwrap();

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    let expected = hex::encode(hasher.finalize());

    let result = verify_package_file(&file_path, Some(&expected));
    assert!(result.is_ok());

    std::fs::remove_dir_all(&tmp).unwrap();
}
