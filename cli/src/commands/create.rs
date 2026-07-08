use anyhow::Result;
use mg_ui::info;

/// mg create-<core> — scaffold a new project non-interactively
pub async fn run(core: &str, framework: &str, project_name: &str) -> Result<()> {
    info(&format!("Creating new {} project '{}' with {}", core, project_name, framework));

    let config = crate::wizard::engine::ScaffoldConfig {
        core: core.to_string(),
        sub_type: String::new(),
        frameworks: vec![],
        project_name: project_name.to_string(),
        features: vec![],
        template_dir: std::path::PathBuf::new(),
    };
    crate::scaffold::Scaffolder::scaffold(&config)?;

    let cwd = std::env::current_dir()?;
    let project_dir = cwd.join(project_name);
    let proj_config = mg_config::project::ProjectConfig::new(project_name, core);
    proj_config.save(&project_dir)?;

    info(&format!("Project '{}' created!", project_name));
    info(&format!("  cd {} && mg install", project_name));

    Ok(())
}