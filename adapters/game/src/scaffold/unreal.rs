//! Unreal project scaffolding.

use super::{render_template, TemplateContext};
use mgc_types::MgResult;
use std::path::Path;

/// Scaffold Unreal project với .uproject
pub async fn scaffold(context: TemplateContext, target_dir: &Path) -> MgResult<()> {
    let ctx_map = context.to_map();

    // <ProjectName>.uproject
    let project_file = format!("{}.uproject", context.project_slug);
    let uproject = render_template(UPROJECT_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join(&project_file), uproject)?;

    // mgc.toml
    let mgc_toml = render_template(MGC_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join("mgc.toml"), mgc_toml)?;

    // Source/<ProjectName>/<ProjectName>.Build.cs (stub)
    let source_dir = target_dir.join("Source").join(&context.project_slug);
    std::fs::create_dir_all(&source_dir)?;

    let build_cs = render_template(BUILD_TEMPLATE, &ctx_map);
    let build_file = format!("{}.Build.cs", context.project_slug);
    std::fs::write(source_dir.join(&build_file), build_cs)?;

    Ok(())
}

const UPROJECT_TEMPLATE: &str = r#"{
    "FileVersion": 3,
    "EngineAssociation": "{{unreal_version}}",
    "Category": "",
    "Description": "{{project_name}}",
    "Modules": [
        {
            "Name": "{{project_slug}}",
            "Type": "Runtime",
            "LoadingPhase": "Default"
        }
    ]
}
"#;

const BUILD_TEMPLATE: &str = r#"using UnrealBuildTool;

public class {{project_slug}} : ModuleRules
{
    public {{project_slug}}(ReadOnlyTargetRules Target) : base(Target)
    {
        PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
        
        PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine" });
    }
}
"#;

const MGC_TEMPLATE: &str = r#"name = "{{project_slug}}"
version = "0.1.0"
ecosystem = "game"

[game]
engine = "unreal"
unreal_version = "{{unreal_version}}"

[execution]
architecture = "native-first"
"#;

#[cfg(test)]
#[path = "test/unreal_test.rs"]
mod tests;
