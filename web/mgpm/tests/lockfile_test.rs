#![cfg(test)]

use tempfile::tempdir;

use mgpm_lockfile::lockfile::{LOCKFILE_VERSION, LOCKFILE_VERSION_V1};
use mgpm_lockfile::pipeline::compute_package_integrity;
use mgpm_lockfile::text::{read_text, write_text};
use mgpm_lockfile::{Lockfile, LockfilePackage, PackageResolution};

#[test]
fn test_lockfile_content_hash_blake3() {
    let mut lock = Lockfile::new(1, "https://registry.npmjs.org");
    lock.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc123".to_string()),
            resolved: false,
            resolved_at: None,
        });
    lock.compute_content_hash();
    assert_eq!(lock.metadata.content_hash.len(), 64);
    assert!(lock
        .metadata
        .content_hash
        .chars()
        .all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_lockfile_deterministic_hash() {
    let pkg_a = LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc123".to_string()),
        resolved: false,
        resolved_at: None,
    };
    let pkg_b = LockfilePackage {
        id: "lodash@4.17.21".to_string(),
        name: "lodash".to_string(),
        version: "4.17.21".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-def456".to_string()),
        resolved: false,
        resolved_at: None,
    };

    let mut lock1 = Lockfile::new(1, "npm");
    lock1.add_package(pkg_a.clone());
    lock1.add_package(pkg_b.clone());

    let mut lock2 = Lockfile::new(1, "npm");
    lock2.add_package(pkg_b);
    lock2.add_package(pkg_a);

    lock1.sort_packages();
    lock1.compute_content_hash();
    lock2.sort_packages();
    lock2.compute_content_hash();

    assert_eq!(lock1.metadata.content_hash, lock2.metadata.content_hash);
}

#[test]
fn test_lockfile_migrate_v1_to_v2() {
    let mut lock = Lockfile::new(1, "npm");
    lock.version = LOCKFILE_VERSION_V1;
    lock.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc123".to_string()),
            resolved: false,
            resolved_at: None,
        });
    lock.metadata.content_hash = lock.compute_content_hash_v1();

    lock.migrate_v1_to_v2().unwrap();
    assert_eq!(lock.version, LOCKFILE_VERSION);
    assert_eq!(lock.metadata.content_hash.len(), 64);
}

#[test]
fn test_compute_package_integrity() {
    let data = b"mock-tarball-content-for-testing";
    let integrity = compute_package_integrity(data);
    assert!(integrity.starts_with("sha256-"));
}

#[test]
fn test_lockfile_v1_backward_compat() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mgpm.lock");

    let mut lock = Lockfile::new(1, "npm");
    lock.version = LOCKFILE_VERSION_V1;
    lock.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc123".to_string()),
            resolved: false,
            resolved_at: None,
        });
    lock.metadata.content_hash = lock.compute_content_hash_v1();

    write_text(&lock, &path).unwrap();
    let loaded = read_text(&path).unwrap();
    assert_eq!(loaded.version, LOCKFILE_VERSION);
    assert_eq!(loaded.packages.len(), 1);
    assert_eq!(loaded.packages[0].name, "react");
    assert_eq!(loaded.packages[0].version, "18.2.0");
    assert_eq!(
        loaded.packages[0].integrity.as_deref(),
        Some("sha512-abc123")
    );
}
