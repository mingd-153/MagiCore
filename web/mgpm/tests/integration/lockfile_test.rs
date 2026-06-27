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
    assert_eq!(lock.version, 1);
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
    });

    let found = lock.find_package("react", "18.2.0");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "react");

    let missing = lock.find_package("react", "17.0.0");
    assert!(missing.is_none());
}
