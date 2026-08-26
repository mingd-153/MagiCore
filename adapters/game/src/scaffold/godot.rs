//! Godot project scaffolding.

use super::{render_template, TemplateContext};
use mgc_types::MgResult;
use std::path::Path;

/// Scaffold Godot project với project.godot + Main.tscn
pub async fn scaffold(context: TemplateContext, target_dir: &Path) -> MgResult<()> {
    let ctx_map = context.to_map();

    // project.godot
    let project_godot = render_template(PROJECT_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join("project.godot"), project_godot)?;

    // Main.tscn
    let main_tscn = render_template(MAIN_SCENE_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join("Main.tscn"), main_tscn)?;

    // mgc.toml
    let mgc_toml = render_template(MGC_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join("mgc.toml"), mgc_toml)?;

    Ok(())
}

const PROJECT_TEMPLATE: &str = r#"config_version=5

[application]

config/name="{{project_name}}"
run/main_scene="res://Main.tscn"
config/features=PackedStringArray("4.3")

[rendering]

renderer/rendering_method="gl_compatibility"
"#;

const MAIN_SCENE_TEMPLATE: &str = r#"[gd_scene format=3]

[node name="Main" type="Node2D"]

[node name="Label" type="Label" parent="."]
offset_right = 200.0
offset_bottom = 50.0
text = "Hello from {{project_name}}!"
"#;

const MGC_TEMPLATE: &str = r#"name = "{{project_slug}}"
version = "0.1.0"
ecosystem = "game"

[game]
engine = "godot"

[execution]
architecture = "native-first"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_scaffold_godot() {
        let tmp = tmp();
        let ctx = TemplateContext::new("my-godot-game", crate::engine::GameEngine::Godot);

        scaffold(ctx, tmp.path()).await.unwrap();

        assert!(tmp.path().join("project.godot").exists());
        assert!(tmp.path().join("Main.tscn").exists());
        assert!(tmp.path().join("mgc.toml").exists());

        let project = std::fs::read_to_string(tmp.path().join("project.godot")).unwrap();
        assert!(project.contains("my-godot-game"));
    }
}
