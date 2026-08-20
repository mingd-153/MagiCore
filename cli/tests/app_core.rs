#![allow(clippy::unwrap_used)]

// App core command surface: add/remove/list/update passthrough guards (offline tests).
mod common;

fn write_app_project(dir: &std::path::Path, language: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("mg.toml"),
        format!(
            r#"
name = "app-test"
version = "0.1.0"
ecosystem = "app"

[app]
language = "{language}"
"#
        ),
    )
    .unwrap();
}

#[test]
fn test_app_commands_require_app_project() {
    let dir = common::work_dir();
    let (ok, out) = common::mg_in(&dir, &["list-app"]);
    assert!(!ok, "list-app outside app project must fail");
    assert!(
        out.contains("Cannot detect an app project"),
        "expected detection error, got: {out}"
    );
}

#[test]
fn test_add_app_without_cli_passthrough_edits_manifest() {
    let dir = common::work_dir();
    write_app_project(&dir, "swift");
    std::fs::write(
        dir.join("Package.swift"),
        "// swift-tools-version:5.9\nimport PackageDescription\n",
    )
    .unwrap();
    let (ok, out) = common::mg_in(&dir, &["add-app", "somepkg"]);
    assert!(!ok, "add-app on swift must fail (no CLI add)");
    assert!(
        out.contains("edit Package.swift"),
        "expected manifest hint, got: {out}"
    );
}
