//! `mg create-<core>` — router: core detect → file con (v5: LỆNH = folder, CORE = file).
//!
//! T5: provider = registry starter kit (create-mg-<core>) → fallback local template → wizard.
//! T9a: sau scaffold, tự ghi `.mg.core` marker tại project folder.

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod hardware;
pub mod iot;
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

        "app" => app::run(framework, project_name).await,
        "game" => game::run(framework, project_name).await,
        "ai" => ai::run(framework, project_name).await,
        "clo" => clo::run(framework, project_name).await,
        "iot" => iot::run(framework, project_name).await,
        "cicd" => cicd::run(framework, project_name).await,
        "lib" | "library" => library::run(project_name).await,
        "hardware" => hardware::run(framework, project_name).await,
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
