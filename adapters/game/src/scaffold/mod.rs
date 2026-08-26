//! Game project scaffolding per Q13 template.toml system.
//! Chuyển từ inline strings sang template files.

use crate::engine::GameEngine;
use mgc_types::MgResult;
use std::collections::HashMap;
use std::path::Path;

pub mod bevy;
pub mod godot;
pub mod unity;
pub mod unreal;

/// Template context cho variable substitution
#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub project_slug: String,
    pub project_name: String,
    pub engine: String,
    pub unity_version: Option<String>,
    pub godot_version: Option<String>,
    pub unreal_version: Option<String>,
}

impl TemplateContext {
    pub fn new(project_name: &str, engine: GameEngine) -> Self {
        let project_slug = project_name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();

        TemplateContext {
            project_slug,
            project_name: project_name.to_string(),
            engine: engine.as_str().to_string(),
            unity_version: None,
            godot_version: None,
            unreal_version: None,
        }
    }

    pub fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("project_slug".to_string(), self.project_slug.clone());
        map.insert("project_name".to_string(), self.project_name.clone());
        map.insert("engine".to_string(), self.engine.clone());

        if let Some(ref v) = self.unity_version {
            map.insert("unity_version".to_string(), v.clone());
        }
        if let Some(ref v) = self.godot_version {
            map.insert("godot_version".to_string(), v.clone());
        }
        if let Some(ref v) = self.unreal_version {
            map.insert("unreal_version".to_string(), v.clone());
        }

        map
    }
}

/// Scaffold new game project
pub async fn scaffold_project(
    engine: GameEngine,
    context: TemplateContext,
    target_dir: &Path,
) -> MgResult<()> {
    std::fs::create_dir_all(target_dir)?;

    match engine {
        GameEngine::Bevy => bevy::scaffold(context, target_dir).await?,
        GameEngine::Godot => godot::scaffold(context, target_dir).await?,
        GameEngine::Unity => unity::scaffold(context, target_dir).await?,
        GameEngine::Unreal => unreal::scaffold(context, target_dir).await?,
    }

    Ok(())
}

/// Render template string với context variables
pub fn render_template(template: &str, context: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    for (key, value) in context {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }

    result
}

#[cfg(test)]
mod tests {
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
}
