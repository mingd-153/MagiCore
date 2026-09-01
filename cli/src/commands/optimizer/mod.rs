//! `optimizer/mod.rs` — MagiCore Core-Neutral Optimizer Engine (Runtime Detection + Adapter Pattern)

pub mod adapters;
pub mod detect;
pub mod generators;
pub mod runtime_detect;

use anyhow::Result;
use std::path::Path;

/// Run optimization for project based on Core and Hardware detection — chạy tối ưu hóa dự án dựa trên Core và phát hiện Hardware
/// REFACTORED: Runtime detection → adapter dispatch (no hardcoded language/runtime) — đã refactor: phát hiện runtime → dispatch adapter
pub fn optimize_project(project_root: &Path, core: &str, force: bool) -> Result<()> {
    let hw = detect::HardwareInfo::detect();
    mgc_ui::info(&format!(
        "Detected System: {} ({}), {} Cores, ~{}GB RAM -> Profile: {:?}",
        hw.os, hw.arch, hw.cpu_cores, hw.total_memory_gb, hw.profile
    ));

    // Detect runtimes for this project — phát hiện runtimes cho project này
    let detected_runtimes = runtime_detect::detect_runtimes(project_root, core);
    if detected_runtimes.is_empty()
        || matches!(
            detected_runtimes[0],
            runtime_detect::DetectedRuntime::Unknown
        )
    {
        mgc_ui::warning(&format!(
            "No runtime detected for `{core}` core in {}. Optimizer skipped.",
            project_root.display()
        ));
        return Ok(());
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

#[cfg(test)]
#[path = "test/optimizer.rs"]
mod tests;

#[cfg(test)]
#[path = "test/runtime_detect_test.rs"]
mod runtime_detect_tests;

#[cfg(test)]
#[path = "test/adapters_test.rs"]
mod adapters_tests;
