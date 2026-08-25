//! `mgc create cicd` — wizard + scaffold theo framework (07 §4).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::cicd::CicdWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Registry-first: fetch layer cicd/<fw> nếu chưa có; fetch fail → fallback procedural.
        crate::commands::template::ensure_layer(&format!("cicd/{fw}")).await;
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("CICD project created. Run `mgc deploy` (dry-run) to preview deployment.");
    Ok(())
}
