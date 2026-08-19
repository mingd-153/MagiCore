//! `mg create-hardware <optimizer|bench>` — scaffold thẳng vào project. Phase 7 v5.

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    let mut config = crate::wizard::hardware::HardwareWizard::run();
    config.project_name = project_name.to_string();
    if !framework.is_empty() {
        config.frameworks = vec![framework.to_string()];
    }
    match config.frameworks.first().map(|s| s.as_str()) {
        Some("optimizer") | Some("bench") => {}
        Some(other) => return Err(crate::error::unknown_hardware_framework(other)),
        None => return Err(crate::error::no_hardware_framework()),
    }
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    mg_ui::success(&format!(
        "Hardware '{}' scaffolded at '{project_name}'. Run `mg add-hardware bench` or `mg bench` to run benchmarks.",
        config.frameworks.first().map(|s| s.as_str()).unwrap_or("")
    ));
    Ok(())
}
