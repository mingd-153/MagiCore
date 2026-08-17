// AI core command surface: remove/list/update real passthrough guards (offline tests).
mod common;

fn write_ai_project(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("mg.toml"),
        r#"
name = "ai-test"
version = "0.1.0"
ecosystem = "ai"

[ai]
framework = "python-agent"
"#,
    )
    .unwrap();
}

#[test]
fn test_remove_ai_without_packages_errors_clearly() {
    let dir = common::work_dir();
    write_ai_project(&dir);
    let (ok, out) = common::mg_in(&dir, &["remove-ai"]);
    assert!(!ok, "remove-ai with no packages must fail");
    assert!(
        out.contains("mg remove-ai <pkg>"),
        "expected usage hint, got: {out}"
    );
}

#[test]
fn test_ai_commands_require_ai_project() {
    let dir = common::work_dir();
    let (ok, out) = common::mg_in(&dir, &["list-ai"]);
    assert!(!ok, "list-ai outside ai project must fail");
    assert!(
        out.contains("Cannot detect an ai project"),
        "expected detection error, got: {out}"
    );
}
