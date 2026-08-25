//! `mgc create-<clo>` — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::cloud::CloudWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Registry-first: fetch layer clo/<fw> nếu chưa có; fetch fail → fallback procedural.
        crate::commands::template::ensure_layer(&format!("clo/{fw}")).await;
    }
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    mgc_ui::success("Cloud project created. Run `mgc add-clo <pkg>` or `mgc install-clo` next.");
    Ok(())
}

#[cfg(test)]
#[path = "test/clo.rs"]
mod tests;
