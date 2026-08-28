#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Install module integration tests.

#![allow(clippy::unwrap_used)]
use mgc_lib_adapter::install::fetch::{crate_tarball_url, pypi_package_index_url};
use mgc_lib_adapter::install::verify::{verify_cargo_lock, verify_python_package};
use mgc_types::{PackageId, PackageName, Version};

#[test]
fn crate_tarball_url_format() {
    let pkg_id = PackageId::new(
        PackageName::new("serde").unwrap(),
        Version::parse("1.0.210").unwrap(),
    );
    let url = crate_tarball_url(&pkg_id);

    assert_eq!(
        url,
        "https://crates.io/api/v1/crates/serde/1.0.210/download"
    );
}

#[test]
fn pypi_package_index_url_format() {
    let url = pypi_package_index_url("requests");
    assert_eq!(url, "https://pypi.org/simple/requests/");
}

#[test]
fn verify_cargo_lock_missing_file() {
    let tmp = std::env::temp_dir().join(format!("mgc-verify-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let result = verify_cargo_lock(&tmp);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cargo.lock"));

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn verify_cargo_lock_exists_passes() {
    let tmp = std::env::temp_dir().join(format!("mgc-lock-ok-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("Cargo.lock"), "# Cargo.lock\n").unwrap();

    let result = verify_cargo_lock(&tmp);
    // Currently trusts cargo's verification, so should pass
    assert!(result.is_ok());

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn verify_python_package_no_hash_warns_but_passes() {
    let tmp = std::env::temp_dir().join(format!("mgc-py-pkg-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let pkg_path = tmp.join("test.whl");
    std::fs::write(&pkg_path, b"fake wheel content").unwrap();

    // No hash provided - should warn but pass
    let result = verify_python_package(&pkg_path, None);
    assert!(result.is_ok());

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn verify_python_package_with_matching_hash() {
    let tmp = std::env::temp_dir().join(format!("mgc-py-hash-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let pkg_path = tmp.join("test.whl");
    let content = b"test content for hash";
    std::fs::write(&pkg_path, content).unwrap();

    // Compute expected SHA-256
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    let expected = hex::encode(hasher.finalize());

    let result = verify_python_package(&pkg_path, Some(&expected));
    assert!(result.is_ok());

    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn verify_python_package_with_mismatched_hash() {
    let tmp = std::env::temp_dir().join(format!("mgc-py-mismatch-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let pkg_path = tmp.join("test.whl");
    std::fs::write(&pkg_path, b"actual content").unwrap();

    let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let result = verify_python_package(&pkg_path, Some(wrong_hash));

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("mismatch"));

    std::fs::remove_dir_all(&tmp).unwrap();
}
