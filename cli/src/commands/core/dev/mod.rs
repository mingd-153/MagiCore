//! `mg dev` — router: core detect → file con (v5: LỆNH = folder, CORE = file).
//! Port cố định per core từ bảng tập trung `dev_port.rs` (RULE §13 — hoán vị 4·3·1·5).

use anyhow::Result;

#[cfg(feature = "ai")]
pub mod ai;
#[cfg(feature = "ai")]
pub mod ai_docker;
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "cicd")]
pub mod cicd;
#[cfg(feature = "clo")]
pub mod clo;
#[cfg(feature = "iot")]
pub mod iot;
pub mod web;

/// Run `mg dev <core>` — lookup port từ bảng, kiểm tra conflict, dispatch đến file con.
///
/// `port_override`: từ flag `--port` của user.
/// `dry_run`: hiển thị lệnh sẽ chạy mà không thực thi thật.
pub async fn run(core: &str, dry_run: bool) -> Result<()> {
    run_with_port(core, dry_run, None).await
}

/// Variant có port override — dùng khi dispatch truyền `--port` flag.
pub async fn run_with_port(core: &str, dry_run: bool, port_override: Option<u16>) -> Result<()> {
    use crate::commands::core::dev_port;

    // Kiểm tra và resolve port
    let port = dev_port::resolve_port(core, port_override);

    match core {
        #[cfg(feature = "ai")]
        "ai" => ai::dev(dry_run).await,
        #[cfg(not(feature = "ai"))]
        "ai" => Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        "clo" | "cloud" => clo::dev(dry_run).await,
        #[cfg(not(feature = "clo"))]
        "clo" | "cloud" => Err(crate::error::core_not_in_build("clo")),
        "web" => {
            let root = std::env::current_dir()?;
            // Truyền port vào web dev (web đã có port 4315 hardcode; đây là override)
            web::dev_at_root(&root, None, port).await
        }
        #[cfg(feature = "cicd")]
        "cicd" => cicd::dev(dry_run).await,
        #[cfg(not(feature = "cicd"))]
        "cicd" => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "app")]
        "app" => app::dev(dry_run).await,
        #[cfg(not(feature = "app"))]
        "app" => Err(crate::error::core_not_in_build("app")),
        "game" => {
            // game chưa có file riêng — hiển thị thông báo
            if dry_run {
                mg_ui::info(&format!(
                    "[dry-run] mg dev game  (port: {})",
                    port.map(|p| p.to_string()).unwrap_or_else(|| "N/A".into())
                ));
                return Ok(());
            }
            mg_ui::info(&format!(
                "mg dev game — port {} (cargo run / godot / unity per engine detect)",
                port.map(|p| p.to_string()).unwrap_or_else(|| "none".into())
            ));
            Ok(())
        }
        #[cfg(feature = "iot")]
        "iot" => {
            if dry_run {
                mg_ui::info(&format!(
                    "[dry-run] mg dev iot  (port: {})",
                    port.map(|p| p.to_string()).unwrap_or_else(|| "N/A".into())
                ));
                return Ok(());
            }
            mg_ui::info("mg dev iot — run `mg flash` to flash firmware (no local server for IoT)");
            Ok(())
        }
        #[cfg(not(feature = "iot"))]
        "iot" => Err(crate::error::core_not_in_build("iot")),
        "lib" | "library" => {
            if dry_run {
                mg_ui::info(&format!(
                    "[dry-run] mg dev lib  (port: {})",
                    port.map(|p| p.to_string()).unwrap_or_else(|| "N/A".into())
                ));
                return Ok(());
            }
            // lib: cargo watch hoặc tsc --watch
            mg_ui::info(&format!(
                "mg dev lib — build-watch server on port {}",
                port.map(|p| p.to_string()).unwrap_or_else(|| "none".into())
            ));
            Ok(())
        }
        other => Err(crate::error::unknown_core(other)),
    }
}

/// Chạy nhiều core song song — kiểm tra conflict port trước rồi spawn từng process.
/// `cores`: Vec<(core_name, port_override)>
pub async fn run_multi(cores: Vec<(&str, Option<u16>)>, dry_run: bool) -> Result<()> {
    use crate::commands::core::dev_port;

    // Kiểm tra conflict trước khi spawn bất kỳ core nào
    let conflicts = dev_port::check_multi_core_conflicts(&cores);
    if !conflicts.is_empty() {
        for (a, b, port) in &conflicts {
            mg_ui::warning(&format!(
                "port conflict: core `{a}` and `{b}` both want port {port}.\n  \
                 → Use `--port` to assign different ports before running both."
            ));
        }
        return Err(anyhow::anyhow!(
            "port conflicts detected — resolve before running multi-core dev"
        ));
    }

    // Spawn từng core
    for (core, port_override) in &cores {
        // Trong thực tế sẽ tokio::spawn để chạy song song;
        // hiện tại báo cáo lần lượt (phase 7 scope: single-core default)
        if dry_run {
            let port = port_override
                .or_else(|| dev_port::default_port(core))
                .map(|p| p.to_string())
                .unwrap_or_else(|| "N/A".into());
            mg_ui::info(&format!("[dry-run] mg dev {core}  (port: {port})"));
        } else {
            run_with_port(core, false, *port_override).await?;
        }
    }
    Ok(())
}
