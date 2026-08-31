//! `mgc create-<ai>` — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::ai::AiWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Phase 3: Handle typed result, không bỏ qua
        match crate::commands::template::ensure_layer(&format!("ai/{fw}")).await {
            Ok(status) if status.is_available() => {
                // Layer OK - proceed
            }
            Ok(_) => {
                mgc_ui::warning(&format!(
                    "Optional AI layer 'ai/{}' not found, using fallback",
                    fw
                ));
            }
            Err(e) => {
                anyhow::bail!("Required AI template layer missing: {}", e);
            }
        }
    }
    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success(
        "AI project created. Pull a model with `mgc model pull hf://...` or run `mgc dev`.",
    );
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
