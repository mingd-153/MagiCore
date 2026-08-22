//! Tests for migrate module
//! Tests cho module migrate

use mg_lockfile::migrate::{auto_upgrade_lockfile, detect_lockfile_version, migrate_v1_to_v2, parse_lockfile_v1, LockfileV1, PackageV1};

#[test]
fn test_detect_version_v1() {
    let toml = r#"
version = "1"
[[package]]
name = "react"
version = "18.2.0"
resolved = "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
"#;
    
    let version = detect_lockfile_version(toml).unwrap();
    assert_eq!(version, 1);
}

#[test]
fn test_detect_version_v2() {
    let toml = r#"
version = "2"
[metadata]
generated_at = "2026-08-21T18:30:00+07:00"
generator = "mg/1.0.0"
lockfile_hash = ""
"#;
    
    let version = detect_lockfile_version(toml).unwrap();
    assert_eq!(version, 2);
}

#[test]
fn test_parse_lockfile_v1() {
    let toml = r#"
version = "1"
[[package]]
name = "react"
version = "18.2.0"
resolved = "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
dependencies = []
"#;
    
    let lockfile_v1 = parse_lockfile_v1(toml).unwrap();
    assert_eq!(lockfile_v1.version, "1");
    assert_eq!(lockfile_v1.packages.len(), 1);
    assert_eq!(lockfile_v1.packages[0].name, "react");
}

#[test]
fn test_migrate_v1_to_v2() {
    let lockfile_v1 = LockfileV1 {
        version: "1".to_string(),
        packages: vec![PackageV1 {
            name: "react".to_string(),
            version: "18.2.0".to_string(),
            resolved: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            dependencies: vec![],
        }],
    };
    
    let lockfile_v2 = migrate_v1_to_v2(lockfile_v1).unwrap();
    assert_eq!(lockfile_v2.version, "2");
    assert_eq!(lockfile_v2.packages.len(), 1);
    assert_eq!(lockfile_v2.packages[0].name, "react");
    assert!(lockfile_v2.packages[0].integrity.starts_with("blake3-"));
}

#[test]
fn test_auto_upgrade_v1() {
    let toml_v1 = r#"
version = "1"
[[package]]
name = "react"
version = "18.2.0"
resolved = "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
dependencies = []
"#;
    
    let lockfile = auto_upgrade_lockfile(toml_v1).unwrap();
    assert_eq!(lockfile.version, "2");
    assert_eq!(lockfile.packages.len(), 1);
}

#[test]
fn test_auto_upgrade_v2_passthrough() {
    let toml_v2 = r#"
version = "2"
[metadata]
generated_at = "2026-08-21T18:30:00+07:00"
generator = "mg/1.0.0"
lockfile_hash = ""
[[package]]
name = "react"
version = "18.2.0"
resolved = "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
integrity = "blake3-abc123"
dependencies = []
"#;
    
    let lockfile = auto_upgrade_lockfile(toml_v2).unwrap();
    assert_eq!(lockfile.version, "2");
    assert_eq!(lockfile.packages.len(), 1);
}
