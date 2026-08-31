//! `mgc create cicd` — wizard + scaffold theo framework (07 §4).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    // Phase 4: Parse scaffold spec sớm với typo detection
    use crate::scaffold::spec::{parse_scaffold_spec, CoreKind};
    if !framework.is_empty() {
        let _spec = parse_scaffold_spec(CoreKind::Cicd, framework).map_err(|e| {
            anyhow::anyhow!("Invalid CI/CD framework specification '{}': {}", framework, e)
        })?;
    }

    let mut config = crate::wizard::cicd::CicdWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Phase 3: Handle typed result
        match crate::commands::template::ensure_layer(&format!("cicd/{fw}")).await {
            Ok(status) if status.is_available() => {}
            Ok(_) => {
                mgc_ui::warning(&format!(
                    "Optional cicd layer 'cicd/{}' not found, using fallback",
                    fw
                ));
            }
            Err(e) => anyhow::bail!("Required cicd template layer missing: {}", e),
        }
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("CICD project created. Run `mgc deploy` (dry-run) to preview deployment.");
    Ok(())
}
