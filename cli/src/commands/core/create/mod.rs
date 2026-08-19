//! `mg create-<core>` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

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

pub async fn run(core: &str, framework: &str, project_name: &str) -> Result<()> {
    match core {
        "web" => {
            // Forward tới web create với cờ mặc định
            let flags = crate::commands::core::scaffold_flags::ScaffoldFlags::default();
            web::run_create_with_options(framework, project_name, Some(flags)).await
        }

        "app" => app::run(framework, project_name).await,
        "game" => game::run(framework, project_name).await,
        "ai" => ai::run(framework, project_name).await,
        "clo" => clo::run(framework, project_name).await,
        "iot" => iot::run(framework, project_name).await,
        "cicd" => cicd::run(framework, project_name).await,
        "lib" | "library" => library::run(project_name).await,
        "hardware" => hardware::run(framework, project_name).await,
        other => Err(crate::error::unknown_core(other)),
    }
}

