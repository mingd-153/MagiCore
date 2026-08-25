//! `mgc create-<game>` — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::game::GameWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Registry-first: fetch layer game/<fw> nếu chưa có (template.toml+sources);
        // fetch fail → fallback generator procedural sẵn có.
        crate::commands::template::ensure_layer(&format!("game/{fw}")).await;
    }
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    mgc_ui::success("Game project created. Run `mgc add-game <pkg>` or `mgc install-game` next.");
    Ok(())
}

#[cfg(test)]
#[path = "test/game.rs"]
mod tests;
