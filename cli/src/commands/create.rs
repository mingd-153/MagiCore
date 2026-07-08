use anyhow::Result;
use mg_ui::info;

/// mg create-<core> — scaffold a new project non-interactively
pub async fn run(core: &str, framework: &str, project_name: &str) -> Result<()> {
    info(&format!(
        "Creating new {} project '{}' with {}",
        core, project_name, framework
    ));

    let config = if core == "web" {
        crate::scaffold::Scaffolder::infer_web_create_config(framework, project_name)?
    } else {
        crate::wizard::engine::ScaffoldConfig {
            core: core.to_string(),
            sub_type: String::new(),
            frameworks: vec![framework.to_string()],
            project_name: project_name.to_string(),
            features: vec![],
            template_dir: std::path::PathBuf::new(),
        }
    };
    let project_dir = crate::scaffold::Scaffolder::scaffold(&config)?;

    let proj_config = mg_config::project::ProjectConfig::new(
        crate::scaffold::Scaffolder::display_name(&project_dir),
        core,
    );
    proj_config.save(&project_dir)?;

    info(&format!("Project '{}' created!", project_dir.display()));
    info(&format!("  cd {} && mg install", project_name));

    Ok(())
}
