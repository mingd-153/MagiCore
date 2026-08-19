//! `mg create iot` — wizard + scaffold theo framework (07 §4).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::iot::IotWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    if let Some(fw) = config.frameworks.first() {
        // Registry-first: fetch layer iot/<fw> nếu chưa có; fetch fail → fallback procedural.
        crate::commands::template::ensure_layer(&format!("iot/{fw}")).await;
    }
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    mg_ui::success("IoT project created. Run `mg add-iot <pkg>` or `mg install-iot` next.");
    Ok(())
}
