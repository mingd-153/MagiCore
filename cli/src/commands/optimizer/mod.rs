//! `optimizer/mod.rs` — MagiCore Hardware-Aware Optimizer Engine.

pub mod detect;
pub mod generators;

use anyhow::Result;
use std::path::Path;

/// Chạy tối ưu hóa dự án dựa trên Core và Hardware detect
pub fn optimize_project(project_root: &Path, core: &str, force: bool) -> Result<()> {
    let hw = detect::HardwareInfo::detect();
    mgc_ui::info(&format!(
        "Detected System: {} ({}), {} Cores, ~{}GB RAM -> Profile: {:?}",
        hw.os, hw.arch, hw.cpu_cores, hw.total_memory_gb, hw.profile
    ));

    let files = generators::generate_optimizations_for_core(core, &hw);
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
