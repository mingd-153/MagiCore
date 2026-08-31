//! `mgc create app` — wizard + scaffold theo framework (07 §4).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    // Phase 4: Parse scaffold spec sớm với typo detection
    use crate::scaffold::spec::{parse_scaffold_spec, CoreKind};
    let parsed_framework = if !framework.is_empty() {
        Some(parse_scaffold_spec(CoreKind::App, framework).map_err(|e| {
            anyhow::anyhow!("Invalid app framework specification '{}': {}", framework, e)
        })?)
    } else {
        None
    };

    let mut config = crate::wizard::app::AppWizard::run();
    config.project_name = project_name.to_string();
    if let Some(spec) = &parsed_framework {
        config.frameworks = vec![spec.normalized_name.clone()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Phase 3: Handle typed result
        match crate::commands::template::ensure_layer(&format!("app/{fw}")).await {
            Ok(status) if status.is_available() => {}
            Ok(_) => {
                mgc_ui::warning(&format!(
                    "Optional app layer 'app/{}' not found, using fallback",
                    fw
                ));
            }
            Err(e) => mgc_ui::warning(&format!(
                "Optional app layer 'app/{}' is unavailable, using fallback: {}",
                fw, e
            )),
        }
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("App project created. Run `mgc install` or `mgc dev` next.");
    Ok(())
}

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
