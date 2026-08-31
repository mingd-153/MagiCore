//! `mgc create library` — wizard + scaffold (07 §4).

use anyhow::Result;

pub async fn run(project_name: &str) -> Result<()> {
    let mut config = crate::wizard::lib::LibWizard::run();
    config.project_name = project_name.to_string();
    if let Some(lang) = config.frameworks.first() {
        // Phase 3: Handle typed result
        match crate::commands::template::ensure_layer(&format!("lib/{lang}")).await {
            Ok(status) if status.is_available() => {}
            Ok(_) => {
                mgc_ui::warning(&format!(
                    "Optional lib layer 'lib/{}' not found, using fallback",
                    lang
                ));
            }
            Err(e) => anyhow::bail!("Required lib template layer missing: {}", e),
        }
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("Library project created. Next: `mgc add-lib` or `mgc install`.");
    Ok(())
}
