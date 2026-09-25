#[test]
fn ai_add_refuses_foreign_python_lock_without_compatibility_escape() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("uv.lock"), "version = 1\n").unwrap();
    let err =
        crate::commands::core::shared::require_native_ai_python(root.path(), "add").unwrap_err();
    assert!(
        err.to_string()
            .contains("No external package manager was invoked")
    );
}

#[test]
fn ai_add_accepts_native_python_manifest() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("pyproject.toml"),
        "[project]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    crate::commands::core::shared::require_native_ai_python(root.path(), "add").unwrap();
}
