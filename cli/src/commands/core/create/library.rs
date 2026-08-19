//! `mg create library` — wizard + scaffold (07 §4).

use anyhow::Result;

pub async fn run(project_name: &str) -> Result<()> {
    let mut config = crate::wizard::lib::LibWizard::run();
    config.project_name = project_name.to_string();
    if let Some(lang) = config.frameworks.first() {
        // Registry-first: fetch layer lib/<lang> nếu chưa có; fetch fail → fallback procedural.
        crate::commands::template::ensure_layer(&format!("lib/{lang}")).await;
    }
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    mg_ui::success("Project created. Next: `mg add-library` or `mg install`.");
    Ok(())
}
