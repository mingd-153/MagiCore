use std::collections::HashMap;
use std::path::Path;

use mg_scaffold::*;

fn create_template(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("package.json.hbs"),
        r#"{"name": "{{name}}", "version": "{{version}}"}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src/index.js.hbs"),
        r#"console.log("Hello {{pascalCase name}}");"#,
    )
    .unwrap();
    std::fs::write(dir.join("README.md"), "# {{name}}").unwrap();
    std::fs::write(dir.join(".gitignore"), "node_modules/").unwrap();
}

fn make_scaffolder(tmp: &tempfile::TempDir, name: &str) -> (StaticScaffolder, ScaffoldContext) {
    let template_dir = tmp.path().join("template");
    create_template(&template_dir);
    let scaffolder = StaticScaffolder::new(template_dir);
    let ctx = ScaffoldContext::new(name, tmp.path().join(name));
    (scaffolder, ctx)
}

#[test]
fn test_create_project_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let (scaffolder, mut ctx) = make_scaffolder(&dir, "my-app");

    ctx.vars.insert("name".to_string(), "my-app".to_string());
    ctx.vars.insert("version".to_string(), "1.0.0".to_string());

    let result = scaffolder.create_project(&ctx, false).unwrap();

    assert_eq!(result.name, "my-app");
    assert_eq!(result.files_created.len(), 4);

    let dest = &ctx.project_path;
    assert!(dest.join("package.json").exists());
    assert!(dest.join("src/index.js").exists());
    assert!(dest.join("README.md").exists());
    assert!(dest.join(".gitignore").exists());
}

#[test]
fn test_create_project_with_force() {
    let dir = tempfile::tempdir().unwrap();
    let (scaffolder, mut ctx) = make_scaffolder(&dir, "test");

    ctx.vars.insert("name".to_string(), "test".to_string());
    ctx.vars.insert("version".to_string(), "1.0.0".to_string());

    scaffolder.create_project(&ctx, false).unwrap();
    let result = scaffolder.create_project(&ctx, true).unwrap();
    assert!(!result.files_created.is_empty());
}

#[test]
fn test_create_project_invalid_name() {
    let dir = tempfile::tempdir().unwrap();
    let template_dir = dir.path().join("template");
    create_template(&template_dir);

    let scaffolder = StaticScaffolder::new(template_dir);
    let ctx = ScaffoldContext::new("", dir.path().join("output"));

    let result = scaffolder.create_project(&ctx, false);
    assert!(result.is_err());
}

#[test]
fn test_create_project_empty_template() {
    let dir = tempfile::tempdir().unwrap();
    let template_dir = dir.path().join("empty");
    std::fs::create_dir(&template_dir).unwrap();

    let scaffolder = StaticScaffolder::new(template_dir);
    let ctx = ScaffoldContext::new("empty-project", dir.path().join("output"));

    let result = scaffolder.create_project(&ctx, false).unwrap();
    assert!(result.files_created.is_empty());
}

#[test]
fn test_create_project_path_exists_no_force() {
    let dir = tempfile::tempdir().unwrap();
    let (scaffolder, ctx) = make_scaffolder(&dir, "existing");

    std::fs::create_dir_all(&ctx.project_path).unwrap();
    std::fs::write(ctx.project_path.join("README.md"), "conflict").unwrap();

    let result = scaffolder.create_project(&ctx, false);
    assert!(matches!(result, Err(ScaffoldError::PathExists(_))));
}

#[test]
fn test_features_wired_through() {
    let dir = tempfile::tempdir().unwrap();
    let templ = dir.path().join("t");
    std::fs::create_dir(&templ).unwrap();
    std::fs::write(templ.join("a.txt"), "x").unwrap();

    let scaffolder = StaticScaffolder::new(templ);
    let mut vars = HashMap::new();
    vars.insert("name".to_string(), "p".to_string());
    let ctx = ScaffoldContext::new("p", dir.path().join("p"))
        .with_vars(vars)
        .with_features(vec!["typescript".to_string(), "tailwindcss".to_string()]);

    let result = scaffolder.create_project(&ctx, false).unwrap();
    assert_eq!(result.features, vec!["typescript", "tailwindcss"]);
}


