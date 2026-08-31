//! `mgc create-<game>` — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    // Phase 4: Parse scaffold spec sớm với typo detection
    use crate::scaffold::spec::{parse_scaffold_spec, CoreKind};
    if !framework.is_empty() {
        let _spec = parse_scaffold_spec(CoreKind::Game, framework).map_err(|e| {
            anyhow::anyhow!("Invalid game framework specification '{}': {}", framework, e)
        })?;
    }

    let mut config = crate::wizard::game::GameWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Phase 3: Handle typed result
        match crate::commands::template::ensure_layer(&format!("game/{fw}")).await {
            Ok(status) if status.is_available() => {}
            Ok(_) => {
                mgc_ui::warning(&format!(
                    "Optional game layer 'game/{}' not found, using fallback generator",
                    fw
                ));
            }
            Err(e) => anyhow::bail!("Required game template layer missing: {}", e),
        }
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success("Game project created. Run `mgc add-game <pkg>` or `mgc install-game` next.");
    Ok(())
}

#[cfg(test)]
#[path = "test/game.rs"]
mod tests;
