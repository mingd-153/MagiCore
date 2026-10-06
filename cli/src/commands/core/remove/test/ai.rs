#[test]
fn ai_remove_refuses_foreign_python_lock_without_native_mutation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("requirements.lock"), "requests==2.0\\n").unwrap();
    let err =
        crate::commands::core::shared::require_native_ai_python(root.path(), "remove").unwrap_err();
    assert!(
        err.to_string()
            .contains("No external package manager was invoked")
    );
}
