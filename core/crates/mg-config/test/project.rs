//! Integration tests for ProjectConfig — test riêng tại test/ (RULE §5)
use mg_config::project::ProjectConfig;
use std::path::PathBuf;

fn temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "megagate-mg-config-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn new_sets_version_and_fields() {
    let cfg = ProjectConfig::new("my-proj", "web");
    assert_eq!(cfg.name, "my-proj");
    assert_eq!(cfg.version, "0.1.0");
    assert_eq!(cfg.ecosystem, "web");
    assert!(cfg.mode.is_empty());
    assert!(cfg.frameworks.is_empty());
    assert_eq!(cfg.execution.architecture, "rust-first");
    assert_eq!(cfg.execution.lane, "compatibility-shell");
    assert_eq!(cfg.execution.compatibility_layer, "js");
}

#[test]
fn from_scaffold_sets_all_fields() {
    let cfg = ProjectConfig::from_scaffold(
        "my-app",
        "web",
        "frontend",
        vec!["react-vite".to_string()],
        "templates/web/frontend/react-vite",
        vec!["ts".to_string(), "tailwind".to_string()],
    );
    assert_eq!(cfg.name, "my-app");
    assert_eq!(cfg.ecosystem, "web");
    assert_eq!(cfg.mode, "frontend");
    assert_eq!(cfg.frameworks, vec!["react-vite"]);
    assert_eq!(cfg.template, "templates/web/frontend/react-vite");
    assert_eq!(cfg.features, vec!["ts", "tailwind"]);
    assert_eq!(cfg.execution.architecture, "rust-first");
    assert_eq!(cfg.execution.lane, "compatibility-shell");
    assert_eq!(cfg.execution.compatibility_layer, "ts");
    assert!(cfg
        .execution
        .native_targets
        .contains(&"frontend-executable".to_string()));
}

#[test]
fn load_missing_returns_none() {
    let dir = temp_test_dir("load-missing");
    assert!(ProjectConfig::load(&dir).unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_valid_file() {
    let dir = temp_test_dir("load-valid");
    let cfg = ProjectConfig::new("test", "web");
    cfg.save(&dir).unwrap();
    let loaded = ProjectConfig::load(&dir).unwrap().unwrap();
    assert_eq!(loaded.name, "test");
    assert_eq!(loaded.ecosystem, "web");
    assert_eq!(loaded.version, "0.1.0");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_invalid_toml_returns_err() {
    let dir = temp_test_dir("load-invalid");
    std::fs::write(dir.join("mg.toml"), "[[[invalid").unwrap();
    assert!(ProjectConfig::load(&dir).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_creates_file_at_root() {
    let dir = temp_test_dir("save-dir");
    let cfg = ProjectConfig::new("save-test", "lib");
    cfg.save(&dir).unwrap();
    let path = dir.join("mg.toml");
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("save-test"));
    assert!(content.contains("lib"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_roundtrip() {
    let dir = temp_test_dir("save-roundtrip");
    let cfg = ProjectConfig::new("roundtrip", "ai");
    cfg.save(&dir).unwrap();
    let loaded = ProjectConfig::load(&dir).unwrap().unwrap();
    assert_eq!(loaded.name, "roundtrip");
    assert_eq!(loaded.ecosystem, "ai");
    assert_eq!(loaded.version, "0.1.0");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn find_root_detects_project_marker() {
    let dir = temp_test_dir("find-root");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    ProjectConfig::new("root-proj", "web").save(&dir).unwrap();
    let found = ProjectConfig::find_project_root(&dir.join("sub")).unwrap();
    assert_eq!(found, dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_roundtrip_with_scaffold_fields() {
    let dir = temp_test_dir("save-scaffold");
    let cfg = ProjectConfig::from_scaffold(
        "roundtrip",
        "web",
        "frontend",
        vec!["react-vite".to_string()],
        "templates/web/frontend/react-vite",
        vec!["ts".to_string()],
    );
    cfg.save(&dir).unwrap();
    let loaded = ProjectConfig::load(&dir).unwrap().unwrap();
    assert_eq!(loaded.name, "roundtrip");
    assert_eq!(loaded.ecosystem, "web");
    assert_eq!(loaded.mode, "frontend");
    assert_eq!(loaded.frameworks, vec!["react-vite"]);
    assert_eq!(loaded.template, "templates/web/frontend/react-vite");
    assert_eq!(loaded.features, vec!["ts"]);
    assert_eq!(loaded.execution.compatibility_layer, "ts");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_detect_package_json_returns_web() {
    let dir = temp_test_dir("auto-web");
    std::fs::write(dir.join("package.json"), "{}").unwrap();
    assert_eq!(ProjectConfig::auto_detect(&dir), Some("web".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_detect_cargo_toml_returns_lib() {
    let dir = temp_test_dir("auto-lib");
    std::fs::write(dir.join("Cargo.toml"), "").unwrap();
    assert_eq!(ProjectConfig::auto_detect(&dir), Some("lib".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_detect_pyproject_toml_returns_ai() {
    let dir = temp_test_dir("auto-ai");
    std::fs::write(dir.join("pyproject.toml"), "").unwrap();
    assert_eq!(ProjectConfig::auto_detect(&dir), Some("ai".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_detect_no_manifest_returns_none() {
    let dir = temp_test_dir("auto-none");
    assert_eq!(ProjectConfig::auto_detect(&dir), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn find_project_root_mg_toml_in_cwd() {
    let dir = temp_test_dir("root-cwd-mg");
    std::fs::write(dir.join("mg.toml"), "").unwrap();
    assert_eq!(ProjectConfig::find_project_root(&dir), Some(dir.clone()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn find_project_root_package_json_in_cwd() {
    let dir = temp_test_dir("root-cwd-pkg");
    std::fs::write(dir.join("package.json"), "{}").unwrap();
    assert_eq!(ProjectConfig::find_project_root(&dir), Some(dir.clone()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn find_project_root_no_match_returns_none() {
    let dir = temp_test_dir("root-none");
    assert_eq!(ProjectConfig::find_project_root(&dir), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn find_project_root_ignores_parent_package_json_without_mg_toml() {
    let root = temp_test_dir("parent-package-json");
    let child = root.join("apps").join("frontend");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(root.join("package.json"), "{}").unwrap();

    assert_eq!(ProjectConfig::find_project_root(&child), None);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn find_project_root_accepts_parent_mg_toml() {
    let root = temp_test_dir("parent-mg-toml");
    let child = root.join("apps").join("frontend");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(root.join("mg.toml"), "").unwrap();

    assert_eq!(ProjectConfig::find_project_root(&child), Some(root.clone()));
    let _ = std::fs::remove_dir_all(root);
}
