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
#[path = "test/mod_test.rs"]
mod tests;
