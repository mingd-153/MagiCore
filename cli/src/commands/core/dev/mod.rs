//! `mg dev` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod web;
pub mod cicd;
pub mod clo;
pub mod iot;

pub async fn run(core: &str, dry_run: bool) -> Result<()> {
    match core {
        "ai" => ai::dev(dry_run).await,
        "clo" => clo::dev(dry_run).await,
        "web" => crate::commands::core::dev::web::dev_at_root(&std::env::current_dir()?, None, None).await,
                "cicd" => crate::commands::core::dev::cicd::dev(dry_run).await,
        "app" => app::dev(dry_run).await,
        other => Err(crate::error::unknown_core(other)),
    }
}
