#[test]
fn ai_update_refuses_foreign_python_lock_without_native_resolver() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("uv.lock"), "version = 1\\n").unwrap();
    let err =
        crate::commands::core::shared::require_native_ai_python(root.path(), "update").unwrap_err();
    assert!(
        err.to_string()
            .contains("No external package manager was invoked")
    );
}
