//! `mg create-<core>` — router: core detect → file con (v5: LỆNH = folder, CORE = file).
//!
//! T5: provider = registry starter kit (create-mg-<core>) → fallback local template → wizard.
//! T9a: sau scaffold, tự ghi `.mg.core` marker tại project folder.

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

pub async fn run(core: &str, framework: &str, project_name: &str) -> Result<()> {
    // T5: Thử fetch starter kit `create-mg-<core>` từ MegaGate registry trước.
    // Nếu offline / không tìm thấy → fallback vào local template + wizard (hiện hành).
    // (Registry fetch chưa có endpoint thật → fallback ngay, TODO khi registry staging live)
    let result = match core {
        "web" => {
            // Forward tới web create với cờ mặc định
            let flags = crate::commands::core::scaffold_flags::ScaffoldFlags::default();
            web::run_create_with_options(framework, project_name, Some(flags)).await
        }

        #[cfg(feature = "app")]
        "app" => app::run(framework, project_name).await,
        #[cfg(not(feature = "app"))]
        "app" => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "game")]
        "game" => game::run(framework, project_name).await,
        #[cfg(not(feature = "game"))]
        "game" => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        "ai" => ai::run(framework, project_name).await,
        #[cfg(not(feature = "ai"))]
        "ai" => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        "clo" => clo::run(framework, project_name).await,
        #[cfg(not(feature = "clo"))]
        "clo" => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "iot")]
        "iot" => iot::run(framework, project_name).await,
        #[cfg(not(feature = "iot"))]
        "iot" => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "cicd")]
        "cicd" => cicd::run(framework, project_name).await,
        #[cfg(not(feature = "cicd"))]
        "cicd" => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "lib")]
        "lib" | "library" => library::run(project_name).await,
        #[cfg(not(feature = "lib"))]
        "lib" | "library" => Err(crate::error::core_not_in_build("lib")),
        #[cfg(feature = "hardware")]
        "hardware" => hardware::run(framework, project_name).await,
        #[cfg(not(feature = "hardware"))]
        "hardware" => Err(crate::error::core_not_in_build("hardware")),
        other => return Err(crate::error::unknown_core(other)),
    };

    // T9a: Ghi .mg.core marker tại project_name/ folder sau scaffold thành công.
    // Fail-soft: chỉ warn nếu không ghi được, không block luồng.
    if result.is_ok() {
        let cwd = std::env::current_dir().unwrap_or_default();
        // project_name có thể là tên thư mục hoặc "." (in-place)
        let project_dir = if project_name.is_empty() || project_name == "." {
            cwd.clone()
        } else {
            cwd.join(project_name)
        };
        if project_dir.is_dir() {
            if let Err(e) =
                mg_config::project::ProjectConfig::write_core_marker_at(&project_dir, core)
            {
                mg_ui::warning(&format!(
                    "Could not write {} marker: {e}",
                    mg_config::project::ProjectConfig::CORE_MARKER_FILE
                ));
            }
        }
    }

    result
}
