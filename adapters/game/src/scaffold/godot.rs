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
#[path = "test/godot_test.rs"]
mod tests;
