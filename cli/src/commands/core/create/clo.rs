//! `mgc create-<clo>` — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    // Phase 4: Parse scaffold spec sớm với typo detection
    use crate::scaffold::spec::{CoreKind, parse_scaffold_spec};
    if !framework.is_empty() {
        let _spec = parse_scaffold_spec(CoreKind::Cloud, framework).map_err(|e| {
            anyhow::anyhow!(
                "Invalid cloud framework specification '{}': {}",
                framework,
                e
            )
        })?;
    }

    let mut config = crate::wizard::cloud::CloudWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Phase 3: Handle typed result
        // Layer namespace is `cloud/` (CoreKind::from_str_core knows
        // "cloud", not the CLI alias "clo" — resolver maps Cloud →
        // "cloud/..."; using "clo/" here fails every create-clo).
        // (Namespace layer là `cloud/`, không phải alias CLI `clo`.)
        match crate::commands::template::ensure_layer(&format!("cloud/{fw}")).await {
            Ok(status) if status.is_available() => {}
            Ok(_) => {
                mgc_ui::warning(&format!(
                    "Optional cloud layer 'cloud/{}' not found, using fallback",
                    fw
                ));
            }
            Err(e) => {
                // Built-in generator fallback — but ONLY for frameworks
                // this core's processor actually generates; anything else
                // keeps the honest layer-required error (no mislabeled scaffold).
                // (Fallback generator nội bộ — chỉ framework processor hỗ trợ.)
                if crate::scaffold::processors::clo::CloProcessor::supports(fw) {
                    mgc_ui::warning(&format!(
                        "Registry layer unavailable ({e}) — using built-in cloud generator",
                    ));
                } else {
                    anyhow::bail!("Required cloud template layer missing: {}", e)
                }
            }
        }
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("Cloud project created. Run `mgc add-clo <pkg>` or `mgc install-clo` next.");
    Ok(())
}

#[cfg(test)]
#[path = "test/clo.rs"]
mod tests;
