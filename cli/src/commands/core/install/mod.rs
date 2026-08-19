//! `mg install` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod web;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod iot;
pub mod library;
pub mod hardware;

pub async fn run(core: &str, packages: Vec<String>) -> Result<()> {
    // Tự động tối ưu hóa phần cứng và quản lý profile trong .mg-optimizer/ khi user install core
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(Some(root)) = crate::commands::core::shared::find_project_root(&cwd) {
            let _ = crate::commands::optimizer::optimize_project(&root, core, false);
        }
    }

    match core {
        "game" => game::install(packages).await,
        "ai" => ai::install(packages, false).await,
        "web" => web::install(packages, false, false, false, false, false).await,
        "clo" | "cloud" => clo::install(packages, false).await,
        "cicd" => cicd::install(packages, false).await,
        "iot" => iot::install(packages).await,
        "app" => app::install(packages, false).await,
        "lib" | "library" => library::install(packages).await,
        "hardware" => hardware::install(packages).await,
        other => Err(crate::error::unknown_core(other)),
    }
}