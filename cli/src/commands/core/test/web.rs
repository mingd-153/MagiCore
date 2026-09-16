#[test]
fn v3_web_lockfile_is_written_with_current_schema_version() {
    // Deliberate v3 bump (Phase 1): web writes target the v3 schema.
    // Nâng lên v3 có chủ đích (Phase 1): web ghi ra nhắm schema v3.
    let lock = mgc_lockfile::Lockfile::new();
    let encoded = mgc_lockfile::serialization::to_toml(&lock).unwrap();
    assert!(encoded.contains("version = \"3\""));
    let decoded: mgc_lockfile::Lockfile = mgc_lockfile::serialization::from_toml(&encoded).unwrap();
    assert_eq!(decoded.version, "3");
}
