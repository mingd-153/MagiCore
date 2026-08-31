//! `mgc create iot` — wizard + scaffold theo framework (07 §4).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::iot::IotWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Phase 3: Handle typed result
        match crate::commands::template::ensure_layer(&format!("iot/{fw}")).await {
            Ok(status) if status.is_available() => {}
            Ok(_) => {
                mgc_ui::warning(&format!(
                    "Optional iot layer 'iot/{}' not found, using fallback",
                    fw
                ));
            }
            Err(e) => anyhow::bail!("Required iot template layer missing: {}", e),
        }
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("IoT project created. Run `mgc add-iot <pkg>` or `mgc install-iot` next.");
    Ok(())
}

#[cfg(test)]
#[path = "test/iot.rs"]
mod tests;
