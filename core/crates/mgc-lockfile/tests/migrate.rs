//! Tests for migrate module
//! Tests cho module migrate

use mgc_lockfile::migrate::{
    LockfileV1, PackageV1, auto_upgrade_lockfile, detect_lockfile_version, migrate_v1_to_v2,
    parse_lockfile_v1,
};

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
generator = "mgc/1.0.0"
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
    // Deliberate v3 bump (Phase 1): auto-upgrade always lands on the latest
    // schema, so the v1 path chains v1→v2→v3.
    // Nâng lên v3 có chủ đích (Phase 1): auto-upgrade luôn về schema mới
    // nhất, nên đường v1 nối chuỗi v1→v2→v3.
    assert_eq!(lockfile.version, "3");
    assert_eq!(lockfile.packages.len(), 1);
    // Chain fills registry-import provenance on the migrated pins.
    // Chuỗi migration điền provenance registry-import cho pin đã migrate.
    assert_eq!(
        lockfile.packages[0]
            .provenance
            .as_ref()
            .unwrap()
            .source_kind,
        "registry-import"
    );
}

#[test]
fn test_auto_upgrade_v2_migrates_to_v3() {
    let toml_v2 = r#"
version = "2"
[metadata]
generated_at = "2026-08-21T18:30:00+07:00"
generator = "mgc/1.0.0"
lockfile_hash = ""
[[package]]
name = "react"
version = "18.2.0"
resolved = "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
integrity = "blake3-abc123"
dependencies = []
"#;

    let lockfile = auto_upgrade_lockfile(toml_v2).unwrap();
    // Deliberate v3 bump (Phase 1): v2 passthrough becomes v2→v3 migration
    // (auto-upgrade targets the latest canonical schema).
    // Nâng lên v3 có chủ đích (Phase 1): passthrough v2 thành migration
    // v2→v3 (auto-upgrade nhắm schema canonical mới nhất).
    assert_eq!(lockfile.version, "3");
    assert_eq!(lockfile.packages.len(), 1);
    assert_eq!(
        lockfile.packages[0].ecosystem,
        mgc_lockfile::ecosystem_tag::EcosystemTag::Other
    );
    assert_eq!(
        lockfile.packages[0]
            .provenance
            .as_ref()
            .unwrap()
            .source_kind,
        "registry-import"
    );
}
