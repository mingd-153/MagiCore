//! `optimizer/mod.rs` — MagiCore Core-Neutral Optimizer Engine (Runtime Detection + Adapter Pattern)

pub mod adapters;
pub mod detect;
pub mod env_loader;
pub mod generators;
pub mod runtime_detect;

use anyhow::Result;
use std::path::Path;

/// Run optimization for project based on Core and Hardware detection — chạy tối ưu hóa dự án dựa trên Core và phát hiện Hardware
/// REFACTORED: Runtime detection → adapter dispatch (no hardcoded language/runtime) — đã refactor: phát hiện runtime → dispatch adapter
pub fn optimize_project(project_root: &Path, core: &str, force: bool) -> Result<()> {
    let hw = detect::HardwareInfo::detect();
    if hw.total_memory_gb == 0 {
        mgc_ui::warning(
            "RAM size could not be detected on this machine — profile degraded to Constrained and memory-derived tuning is skipped (no fabricated values).",
        );
    }
    mgc_ui::info(&format!(
        "Detected System: {} ({}), {} Cores, {} RAM -> Profile: {:?}",
        hw.os,
        hw.arch,
        hw.cpu_cores,
        if hw.total_memory_gb == 0 {
            "unknown".to_string()
        } else {
            format!("~{}GB", hw.total_memory_gb)
        },
        hw.profile
    ));

    // Detect runtimes for this project — phát hiện runtimes cho project này
    let detected_runtimes = runtime_detect::detect_runtimes(project_root, core);
    if detected_runtimes.is_empty()
        || matches!(
            detected_runtimes[0],
            runtime_detect::DetectedRuntime::Unknown
        )
    {
        // Fail closed: automation reading Ok(()) here would report a
        // successful optimization that never happened (V1.2: no success
        // without verifiable state change).
        // (Fail-closed: automation đọc Ok(()) ở đây sẽ báo tối ưu thành
        // công trong khi chưa có gì xảy ra.)
        anyhow::bail!(
            "No runtime detected for `{core}` core in {} — nothing was optimized (not a success).",
            project_root.display()
        );
    }

    mgc_ui::info(&format!(
        "Detected runtimes for `{core}`: {:?}",
        detected_runtimes
    ));

    // Generate optimizations via adapters — tạo optimizations qua adapters
    let files = generators::generate_optimizations_for_core(core, &hw, project_root);
    if files.is_empty() {
        mgc_ui::info(&format!(
            "No specific hardware optimizations needed for `{core}` core."
        ));
        return Ok(());
    }

    let mut applied_count = 0;
    for file in &files {
        if generators::apply_optimized_file(project_root, file, force)? {
            applied_count += 1;
        }
    }

    mgc_ui::success(&format!(
        "MagiCore Optimizer finished: applied {applied_count}/{} configurations for `{core}`.",
        files.len()
    ));
    Ok(())
}

/// CLI entry point for `mgc optimizer` command — điểm vào CLI cho lệnh `mgc optimizer`
/// Run optimizer on current project, auto-detect core from .mgc.core or use --core flag
pub async fn run(core: Option<&str>, force: bool) -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(core)?;
    let project_root = ctx.root();
    let ecosystem = ctx.adapter().ecosystem();
    let detected_core = match ecosystem {
        mgc_types::Ecosystem::Web => "web",
        mgc_types::Ecosystem::Game => "game",
        mgc_types::Ecosystem::Ai => "ai",
        mgc_types::Ecosystem::Cloud => "cloud",
        mgc_types::Ecosystem::Cicd => "cicd",
        mgc_types::Ecosystem::Iot => "iot",
        mgc_types::Ecosystem::App => "app",
        mgc_types::Ecosystem::Lib => "lib",
        mgc_types::Ecosystem::Hardware => "hardware",
    };

    mgc_ui::info(&format!(
        "Running MagiCore Optimizer for `{}` core in {}",
        detected_core,
        project_root.display()
    ));

    optimize_project(project_root, detected_core, force)
}

#[cfg(test)]
#[path = "test/optimizer.rs"]
mod tests;

#[cfg(test)]
#[path = "test/runtime_detect_test.rs"]
mod runtime_detect_tests;

#[cfg(test)]
#[path = "test/adapters_test.rs"]
mod adapters_tests;
