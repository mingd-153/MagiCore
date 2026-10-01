//! Regression coverage for the legacy checksum writer's fail-closed behavior.

#[test]
fn unsupported_checksum_write_does_not_report_success_or_create_artifacts() {
    let project = tempfile::tempdir().unwrap();
    let error = mgc_lockfile::write_lockfile_checksum(project.path(), b"lock bytes")
        .expect_err("the legacy checksum helper must not claim a write it does not perform");

    assert!(error.to_string().contains("checksum"));
    assert!(error.to_string().contains("signed lockfile API"));
    assert_eq!(std::fs::read_dir(project.path()).unwrap().count(), 0);
}
