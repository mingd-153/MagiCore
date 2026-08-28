use crate::commands::run::{resolve_mgc_toml_script, resolve_package_json_script};
use std::path::PathBuf;

fn write(path: PathBuf, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn mgc_toml_script_resolves_by_name() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("mgc.toml"),
        "[scripts]\nstart = \"mgc dev --port 3000\"\ntest = \"cargo test\"\n",
    );
    let resolved = resolve_mgc_toml_script(&dir.path().join("mgc.toml"), "test").unwrap();
    assert_eq!(resolved.as_deref(), Some("cargo test"));
    assert_eq!(
        resolve_mgc_toml_script(&dir.path().join("mgc.toml"), "missing").unwrap(),
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
fn mgc_toml_script_has_priority_source() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("mgc.toml"),
        "[scripts]\nrun = \"echo mgc-toml-wins\"\n",
    );
    write(
        dir.path().join("package.json"),
        r#"{"scripts": {"run": "echo package-json-loses"}}"#,
    );
    let mgc = resolve_mgc_toml_script(&dir.path().join("mgc.toml"), "run").unwrap();
    let pkg = resolve_package_json_script(&dir.path().join("package.json"), "run").unwrap();
    assert_eq!(mgc.as_deref(), Some("echo mgc-toml-wins"));
    assert_eq!(pkg.as_deref(), Some("echo package-json-loses"));
}
