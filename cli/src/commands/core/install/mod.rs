//! `mg install` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

#[cfg(feature = "ai")]
pub mod ai;
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "cicd")]
pub mod cicd;
#[cfg(feature = "clo")]
pub mod clo;
#[cfg(feature = "game")]
pub mod game;
#[cfg(feature = "hardware")]
pub mod hardware;
#[cfg(feature = "iot")]
pub mod iot;
#[cfg(feature = "lib")]
pub mod library;
pub mod web;

pub async fn run(core: &str, packages: Vec<String>) -> Result<()> {
    // Tự động tối ưu hóa phần cứng và quản lý profile trong .mg-optimizer/ khi user install core
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(Some(root)) = crate::commands::core::shared::find_project_root(&cwd) {
            let _ = crate::commands::optimizer::optimize_project(&root, core, false);
        }
    }

    match core {
        #[cfg(feature = "game")]
        "game" => game::install(packages).await,
        #[cfg(not(feature = "game"))]
        "game" => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        "ai" => ai::install(packages, false).await,
        #[cfg(not(feature = "ai"))]
        "ai" => Err(crate::error::core_not_in_build("ai")),
        "web" => web::install(packages, false, false, false, false, false).await,
        #[cfg(feature = "clo")]
        "clo" | "cloud" => clo::install(packages, false).await,
        #[cfg(not(feature = "clo"))]
        "clo" | "cloud" => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        "cicd" => cicd::install(packages, false).await,
        #[cfg(not(feature = "cicd"))]
        "cicd" => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        "iot" => iot::install(packages).await,
        #[cfg(not(feature = "iot"))]
        "iot" => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        "app" => app::install(packages, false).await,
        #[cfg(not(feature = "app"))]
        "app" => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        "lib" | "library" => library::install(packages).await,
        #[cfg(not(feature = "lib"))]
        "lib" | "library" => Err(crate::error::core_not_in_build("lib")),
        #[cfg(feature = "hardware")]
        "hardware" => hardware::install(packages).await,
        #[cfg(not(feature = "hardware"))]
        "hardware" => Err(crate::error::core_not_in_build("hardware")),
        other => Err(crate::error::unknown_core(other)),
    }
}
