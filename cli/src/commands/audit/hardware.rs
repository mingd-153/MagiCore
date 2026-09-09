//! Hardware core audit — dispatch to the adapter's real scanner via the
//! shared pipeline (detect → adapter → scanner → typed report → exit
//! contract). Template packages carry no dependency graph, so the scanner
//! reports honestly Unavailable: strict CI fails (P0-7), local exits 0
//! only with a loud UNVERIFIED warning.
//! Audit core Hardware — dispatch qua pipeline chung; template package
//! không có dependency graph nên scanner trung thực Unavailable: strict CI
//! fail (P0-7), local chỉ exit 0 kèm cảnh báo UNVERIFIED rõ ràng.

use super::{StrictMode, finish_and_print, run_adapter_audit};
use crate::context::ProjectContext;

pub async fn audit(ctx: &ProjectContext) -> anyhow::Result<()> {
    let report = run_adapter_audit(ctx).await?;

    let pkgs = ctx.adapter().list(ctx.root()).await?;
    println!("  ℹ   hardware packages: {}", pkgs.len());
    for p in &pkgs {
        println!("      {}@{}", p.id.name().as_str(), p.id.version());
    }

    // Shared fail-closed finisher — no more silent exit-0 on unavailable.
    // Finisher fail-closed chung — hết exit 0 âm thầm khi unavailable.
    finish_and_print("hardware", &report, StrictMode::from_env()).await
}
