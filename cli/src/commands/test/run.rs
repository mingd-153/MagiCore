use crate::commands::run::{resolve_mg_toml_script, resolve_package_json_script};
use std::path::PathBuf;

fn write(path: PathBuf, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn mg_toml_script_resolves_by_name() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("mg.toml"),
        "[scripts]\nstart = \"mg dev --port 3000\"\ntest = \"cargo test\"\n",
    );
    let resolved = resolve_mg_toml_script(&dir.path().join("mg.toml"), "test").unwrap();
    assert_eq!(resolved.as_deref(), Some("cargo test"));
    assert_eq!(
        resolve_mg_toml_script(&dir.path().join("mg.toml"), "missing").unwrap(),
        None
    );
}

#[test]
fn package_json_script_resolves_by_name() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("package.json"),
        r#"{"scripts": {"build": "vite build", "dev": "vite --port 5173"}}"#,
    );
    let resolved = resolve_package_json_script(&dir.path().join("package.json"), "build").unwrap();
    assert_eq!(resolved.as_deref(), Some("vite build"));
    assert_eq!(
        resolve_package_json_script(&dir.path().join("package.json"), "nope").unwrap(),
        None
    );
}

#[test]
fn mg_toml_script_has_priority_source() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("mg.toml"),
        "[scripts]\nrun = \"echo mg-toml-wins\"\n",
    );
    write(
        dir.path().join("package.json"),
        r#"{"scripts": {"run": "echo package-json-loses"}}"#,
    );
    let mg = resolve_mg_toml_script(&dir.path().join("mg.toml"), "run").unwrap();
    let pkg = resolve_package_json_script(&dir.path().join("package.json"), "run").unwrap();
    assert_eq!(mg.as_deref(), Some("echo mg-toml-wins"));
    assert_eq!(pkg.as_deref(), Some("echo package-json-loses"));
}
