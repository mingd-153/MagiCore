//! `mg create app` — wizard + scaffold theo framework (07 §4).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::app::AppWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Registry-first: fetch layer app/<fw> nếu chưa có; fetch fail → fallback procedural.
        crate::commands::template::ensure_layer(&format!("app/{fw}")).await;
    }
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    mg_ui::success("App project created. Run `mg install` or `mg dev` next.");
    Ok(())
}

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
