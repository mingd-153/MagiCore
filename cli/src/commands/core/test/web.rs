#[test]
fn v2_web_lockfile_is_written_with_schema_version_two() {
    let lock = mgc_lockfile::Lockfile::new();
    let encoded = mgc_lockfile::serialization::to_toml(&lock).unwrap();
    assert!(encoded.contains("version = \"2\""));
    let decoded: mgc_lockfile::Lockfile = mgc_lockfile::serialization::from_toml(&encoded).unwrap();
    assert_eq!(decoded.version, "2");
}
