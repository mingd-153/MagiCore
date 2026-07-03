//! Integration tests for lockfile round-trip and format conversion

use mgpm_lockfile::{Lockfile, LockfilePackage, PackageResolution};

#[test]
fn test_roundtrip_binary_text() {
    let dir = tempfile::tempdir().unwrap();

    let mut original = Lockfile::new(1, "https://registry.npmjs.org");
    original.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc".to_string()),
        dependencies: vec![],
    });
    original.add_package(LockfilePackage {
        id: "lodash@4.17.21".to_string(),
        name: "lodash".to_string(),
        version: "4.17.21".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-def".to_string()),
        dependencies: vec![],
    });
    original.sort_packages();
    original.compute_content_hash();

    let text_path = dir.path().join("mgpm.lock");
    mgpm_lockfile::text::write_text(&original, &text_path).unwrap();
    let from_text = mgpm_lockfile::text::read_text(&text_path).unwrap();

    let binary_path = dir.path().join("mgpm.lockb");
    mgpm_lockfile::binary::write_binary(&original, &binary_path).unwrap();
    let from_binary = mgpm_lockfile::binary::read_binary(&binary_path).unwrap();

    assert_eq!(from_text.packages.len(), original.packages.len());
    assert_eq!(from_binary.packages.len(), original.packages.len());

    for (a, b) in from_text.packages.iter().zip(from_binary.packages.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.version, b.version);
    }
}

#[test]
fn test_lockfile_validation() {
    let lock = Lockfile::new(1, "npm");
    assert_eq!(lock.version, mgpm_lockfile::LOCKFILE_VERSION);
    assert_eq!(lock.metadata.registry, "npm");
    assert!(lock.packages.is_empty());
}

#[test]
fn test_lockfile_sort() {
    let mut lock = Lockfile::new(1, "npm");
    lock.add_package(LockfilePackage {
        id: "zzz@2.0.0".to_string(),
        name: "zzz".to_string(),
        version: "2.0.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "".to_string(),
            registry: None,
        },
        integrity: None,
        dependencies: vec![],
    });
    lock.add_package(LockfilePackage {
        id: "aaa@1.0.0".to_string(),
        name: "aaa".to_string(),
        version: "1.0.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "".to_string(),
            registry: None,
        },
        integrity: None,
        dependencies: vec![],
    });

    lock.sort_packages();
    assert_eq!(lock.packages[0].name, "aaa");
    assert_eq!(lock.packages[1].name, "zzz");
}

#[test]
fn test_lockfile_find_package() {
    let mut lock = Lockfile::new(1, "npm");
    lock.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "".to_string(),
            registry: None,
        },
        integrity: None,
        dependencies: vec![],
    });

    let found = lock.find_package("react", "18.2.0");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "react");

    let missing = lock.find_package("react", "17.0.0");
    assert!(missing.is_none());
}

#[test]
fn test_migrate_v1_to_v2_roundtrip() {
    let mut lock = Lockfile::new(1, "npm");
    lock.version = mgpm_lockfile::lockfile::LOCKFILE_VERSION_V1;

    lock.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc".to_string()),
        dependencies: vec![],
    });
    lock.metadata.content_hash = lock.compute_content_hash_v1();
    lock.migrate_v1_to_v2().unwrap();

    assert_eq!(lock.version, mgpm_lockfile::LOCKFILE_VERSION);
    assert_eq!(lock.metadata.content_hash.len(), 64);
}

#[test]
fn test_migrate_v1_tampered_rejected() {
    let mut lock = Lockfile::new(1, "npm");
    lock.version = mgpm_lockfile::lockfile::LOCKFILE_VERSION_V1;

    lock.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc".to_string()),
        dependencies: vec![],
    });
    lock.sort_packages();
    lock.metadata.content_hash = "tampered-hash".to_string();

    let result = lock.migrate_v1_to_v2();
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        mgpm_lockfile::LockfileError::ContentHashMismatch { .. }
    ));
}

#[test]
fn test_text_auto_migrates_v1_to_v2() {
    let dir = tempfile::tempdir().unwrap();

    let mut lock = Lockfile::new(1, "npm");
    lock.version = mgpm_lockfile::lockfile::LOCKFILE_VERSION_V1;

    lock.add_package(LockfilePackage {
        id: "lodash@4.17.21".to_string(),
        name: "lodash".to_string(),
        version: "4.17.21".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-xyz".to_string()),
        dependencies: vec![],
    });
    lock.sort_packages();
    lock.metadata.content_hash = lock.compute_content_hash_v1();

    let text_path = dir.path().join("mgpm.lock");
    mgpm_lockfile::text::write_text(&lock, &text_path).unwrap();

    let loaded = mgpm_lockfile::text::read_text(&text_path).unwrap();

    assert_eq!(loaded.version, mgpm_lockfile::LOCKFILE_VERSION);
    assert_eq!(loaded.metadata.content_hash.len(), 64);
    assert_eq!(loaded.packages.len(), 1);
    assert_eq!(loaded.packages[0].name, "lodash");
}

#[test]
fn test_binary_accepts_v1_and_v2() {
    let dir = tempfile::tempdir().unwrap();

    let mut lock = Lockfile::new(1, "npm");
    lock.version = mgpm_lockfile::lockfile::LOCKFILE_VERSION_V1;

    lock.add_package(LockfilePackage {
        id: "typescript@5.4.0".to_string(),
        name: "typescript".to_string(),
        version: "5.4.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/typescript/-/typescript-5.4.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc".to_string()),
        dependencies: vec![],
    });
    lock.sort_packages();
    lock.metadata.content_hash = lock.compute_content_hash_v1();

    let binary_path = dir.path().join("mgpm.lockb");
    mgpm_lockfile::binary::write_binary(&lock, &binary_path).unwrap();

    let loaded = mgpm_lockfile::binary::read_binary(&binary_path).unwrap();
    assert_eq!(loaded.version, mgpm_lockfile::lockfile::LOCKFILE_VERSION_V1);
    assert_eq!(loaded.packages.len(), 1);
}

#[test]
fn test_compute_package_integrity_sri_format() {
    let data = b"test tarball content";
    let integrity = mgpm_lockfile::pipeline::compute_package_integrity(data);
    assert!(
        integrity.starts_with("sha256-"),
        "SRI must start with sha256-"
    );
    let base64_part = integrity.strip_prefix("sha256-").unwrap();
    // SHA-256 = 32 bytes → 43 chars in base64 no-pad
    assert_eq!(
        base64_part.len(),
        43,
        "base64 encoded SHA-256 must be 43 chars"
    );
    // STANDARD_NO_PAD — no '=' padding
    assert!(!base64_part.contains('='), "no padding chars allowed");
}

#[test]
fn test_compute_package_integrity_deterministic() {
    let data = b"hello world";
    let a = mgpm_lockfile::pipeline::compute_package_integrity(data);
    let b = mgpm_lockfile::pipeline::compute_package_integrity(data);
    assert_eq!(a, b);
}

#[test]
fn test_compute_package_integrity_different_inputs() {
    let a = mgpm_lockfile::pipeline::compute_package_integrity(b"package A content");
    let b = mgpm_lockfile::pipeline::compute_package_integrity(b"package B content");
    assert_ne!(a, b);
}
