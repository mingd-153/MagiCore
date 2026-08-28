#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Adapter tests

use super::*;

use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn test_template_context() {
    let ctx = TemplateContext::new("My Game Project", GameEngine::Bevy);

    assert_eq!(ctx.project_slug, "my-game-project");
    assert_eq!(ctx.project_name, "My Game Project");
    assert_eq!(ctx.engine, "bevy");
}

#[test]
fn test_template_context_to_map() {
    let ctx = TemplateContext::new("test", GameEngine::Unity);
    let map = ctx.to_map();

    assert_eq!(map.get("project_slug"), Some(&"test".to_string()));
    assert_eq!(map.get("engine"), Some(&"unity".to_string()));
}

#[test]
fn test_render_template() {
    let mut context = HashMap::new();
    context.insert("name".to_string(), "TestGame".to_string());
    context.insert("version".to_string(), "0.1.0".to_string());

    let template = "name = \"{{name}}\"\nversion = \"{{version}}\"";
    let rendered = render_template(template, &context);

    assert!(rendered.contains("TestGame"));
    assert!(rendered.contains("0.1.0"));
}

#[tokio::test]
async fn test_scaffold_bevy() {
    let tmp = tmp();
    let ctx = TemplateContext::new("test-game", GameEngine::Bevy);

    scaffold_project(GameEngine::Bevy, ctx, tmp.path())
        .await
        .unwrap();

    assert!(tmp.path().join("Cargo.toml").exists());
}
