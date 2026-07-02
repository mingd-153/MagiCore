use std::collections::HashMap;
use std::path::Path;

use mgpm_scaffold::*;

fn create_template(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("package.json.hbs"), r#"{"name": "{{name}}", "version": "{{version}}"}"#).unwrap();
    std::fs::write(dir.join("src/index.js.hbs"), r#"console.log("Hello {{pascalCase name}}");"#).unwrap();
    std::fs::write(dir.join("README.md"), "# {{name}}").unwrap();
    std::fs::write(dir.join(".gitignore"), "node_modules/").unwrap();
}

#[test]
fn test_create_project_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let template_dir = dir.path().join("template");
    create_template(&template_dir);

    let scaffolder = StaticScaffolder::new(template_dir);
    let dest = dir.path().join("my-app");

    let mut vars = HashMap::new();
    vars.insert("name".to_string(), "my-app".to_string());
    vars.insert("version".to_string(), "1.0.0".to_string());

    let result = scaffolder.create_project("my-app", &dest, &vars, false).unwrap();

    assert_eq!(result.name, "my-app");
    assert_eq!(result.files_created.len(), 4);

    assert!(dest.join("package.json").exists());
    assert!(dest.join("src/index.js").exists());
    assert!(dest.join("README.md").exists());
    assert!(dest.join(".gitignore").exists());
}

#[test]
fn test_create_project_with_force() {
    let dir = tempfile::tempdir().unwrap();
    let template_dir = dir.path().join("template");
    create_template(&template_dir);

    let scaffolder = StaticScaffolder::new(template_dir);
    let dest = dir.path().join("my-app");

    let mut vars = HashMap::new();
    vars.insert("name".to_string(), "test".to_string());
    vars.insert("version".to_string(), "1.0.0".to_string());

    scaffolder.create_project("test", &dest, &vars, false).unwrap();
    let result = scaffolder.create_project("test", &dest, &vars, true).unwrap();
    assert!(!result.files_created.is_empty());
}

#[test]
fn test_create_project_invalid_name() {
    let dir = tempfile::tempdir().unwrap();
    let template_dir = dir.path().join("template");
    create_template(&template_dir);

    let scaffolder = StaticScaffolder::new(template_dir);
    let dest = dir.path().join("output");

    let result = scaffolder.create_project("", &dest, &HashMap::new(), false);
    assert!(result.is_err());
}

#[test]
fn test_create_project_empty_template() {
    let dir = tempfile::tempdir().unwrap();
    let template_dir = dir.path().join("empty");
    std::fs::create_dir(&template_dir).unwrap();

    let scaffolder = StaticScaffolder::new(template_dir);
    let dest = dir.path().join("output");

    let result = scaffolder.create_project("empty-project", &dest, &HashMap::new(), false).unwrap();
    assert!(result.files_created.is_empty());
}

#[test]
fn test_create_project_path_exists_no_force() {
    let dir = tempfile::tempdir().unwrap();
    let template_dir = dir.path().join("template");
    create_template(&template_dir);

    let scaffolder = StaticScaffolder::new(template_dir);
    let dest = dir.path().join("existing");
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("README.md"), "conflict").unwrap();

    let result = scaffolder.create_project("test", &dest, &HashMap::new(), false);
    assert!(matches!(result, Err(ScaffoldError::PathExists(_))));
}
