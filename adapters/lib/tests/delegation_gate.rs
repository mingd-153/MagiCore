//! Regression tests for native-only dependency boundaries.
//! Kiểm thử hồi quy cho ranh giới dependency chỉ-native.

#[test]
fn pip_allowlist_cannot_enable_external_package_manager() {
    let project = tempfile::tempdir().expect("create isolated project directory");
    std::fs::write(
        project.path().join("mgc.toml"),
        "[lib]\npip_allowed_packages = [\"requests\"]\n",
    )
    .expect("write project configuration");

    let error = mgc_lib_adapter::check_pip_allowed(project.path(), "requests")
        .expect_err("a config allowlist must not authorize external pip");
    assert!(error.to_string().contains("never invokes pip"));
}
