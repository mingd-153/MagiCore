//! `mgc create library` — wizard + scaffold (07 §4).

use anyhow::Result;

pub async fn run(project_name: &str) -> Result<()> {
    let mut config = crate::wizard::lib::LibWizard::run();
    config.project_name = project_name.to_string();
    if let Some(lang) = config.frameworks.first() {
        // Registry-first: fetch layer lib/<lang> nếu chưa có; fetch fail → fallback procedural.
        crate::commands::template::ensure_layer(&format!("lib/{lang}")).await;
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("Library project created. Next: `mgc add-lib` or `mgc install`.");
    Ok(())
}
