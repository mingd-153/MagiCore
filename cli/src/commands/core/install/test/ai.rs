#[test]
fn ai_install_refuses_requirements_file_without_native_lane() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("requirements.txt"), "requests\\n").unwrap();
    let err = crate::commands::core::shared::require_native_ai_python(root.path(), "install")
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("No external package manager was invoked")
    );
}
