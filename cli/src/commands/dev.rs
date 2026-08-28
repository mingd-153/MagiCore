use mgc_ui::info;

use crate::context::ProjectContext;

/// Lệnh dev cho từng engine game (Q15/Q20): bevy → cargo run; godot → mở editor.
#[cfg(feature = "game")]
fn game_dev_command(root: &std::path::Path) -> anyhow::Result<(String, Vec<String>)> {
    let adapter = mgc_game_adapter::adapter_for(root)
        .ok_or_else(|| crate::error::no_framework_detected("game engine", root))?;
    match adapter.engine() {
        "bevy" => Ok(("cargo".to_string(), vec!["run".to_string()])),
        "godot" => {
            let path = root.to_str().ok_or_else(crate::error::path_not_utf8)?;
            Ok((
                "godot".to_string(),
                vec![
                    "--editor".to_string(),
                    "--path".to_string(),
                    path.to_string(),
                ],
            ))
        }
        // unity: mở editor GUI qua CLI (fail-closed nếu unity không có trong PATH).
        "unity" => {
            let path = root.to_str().ok_or_else(crate::error::path_not_utf8)?;
            Ok((
                "unity".to_string(),
                vec!["-projectPath".to_string(), path.to_string()],
            ))
        }
        // unreal: engine binary đặt tên theo version, không có PATH chuẩn — chỉ dẫn mở editor.
        "unreal" => {
            let uproject = root
                .read_dir()
                .ok()
                .and_then(|mut entries| {
                    entries.find_map(|e| {
                        let name = e.as_ref().ok()?.file_name().to_string_lossy().to_string();
                        name.ends_with(".uproject").then_some(name)
                    })
                })
                .unwrap_or_else(|| "Game.uproject".to_string());
            Err(crate::error::unreal_editor_dev(&uproject))
        }
        other => Err(crate::error::dev_no_command_for_engine(other)),
    }
}

pub async fn run(
    core: Option<&str>,
    host: Option<String>,
    port: Option<u16>,
    clear: bool,
) -> anyhow::Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let host = host.unwrap_or_else(|| "localhost".to_string());
    let root = ctx.root().to_path_buf();

    info("Starting MagiCore Native Dev Server...");
    info(&format!("Project root: {}", root.display()));
    info(&format!("Execution profile: {}", ctx.execution_summary()));

    match ctx.adapter().name() {
        "web" => {
            if clear {
                info("--clear is delegated to the selected web framework when supported.");
            }
            crate::commands::core::dev::web::dev_at_root(&root, Some(host), port).await
        }
        #[cfg(feature = "game")]
        "game" => {
            let (cmd, args) = game_dev_command(&root)?;
            let opts = mgc_exec::prelude::ExecOptions {
                cwd: Some(root.clone()),
                log_path: Some(root.join(".magicore").join("exec.log")),
                clean_env: true,
                ..Default::default()
            };
            info(&format!(
                "Game dev: running `{} {}`...",
                cmd,
                args.join(" ")
            ));
            mgc_exec::prelude::run_inherited(&cmd, &args, &opts).map_err(|e| {
                if cmd == "godot" {
                    crate::error::godot_failed(&e)
                } else {
                    crate::error::tool_failed(&cmd, &e)
                }
            })?;
            Ok(())
        }
        #[cfg(not(feature = "game"))]
        "game" => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "iot")]
        "iot" => {
            let (cmd, args) = iot_dev_command(&root)?;
            let opts = mgc_exec::prelude::ExecOptions {
                cwd: Some(root.clone()),
                log_path: Some(root.join(".magicore").join("exec.log")),
                clean_env: true,
                ..Default::default()
            };
            info(&format!("IoT dev: running `{} {}`...", cmd, args.join(" ")));
            mgc_exec::prelude::run_inherited(&cmd, &args, &opts).map_err(|e| {
                if cmd == "espflash" {
                    crate::error::espflash_failed(&e)
                } else {
                    crate::error::tool_failed(&cmd, &e)
                }
            })?;
            Ok(())
        }
        #[cfg(not(feature = "iot"))]
        "iot" => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "clo")]
        "cloud" => {
            crate::commands::core::dev::clo::dev(false).await?;
            Ok(())
        }
        #[cfg(not(feature = "clo"))]
        "cloud" => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        "cicd" => {
            crate::commands::core::dev::cicd::dev(false).await?;
            Ok(())
        }
        #[cfg(not(feature = "cicd"))]
        "cicd" => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "app")]
        "app" => {
            crate::commands::core::dev::app::dev(false).await?;
            Ok(())
        }
        #[cfg(not(feature = "app"))]
        "app" => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "ai")]
        "ai" => {
            crate::commands::core::dev::ai::dev(false).await?;
            Ok(())
        }
        #[cfg(not(feature = "ai"))]
        "ai" => Err(crate::error::core_not_in_build("ai")),
        other => Err(crate::error::dev_core_not_implemented(other)),
    }
}

/// Lệnh dev cho từng framework IoT (Q16/Q20): esp32-rust → espflash monitor;
/// platformio/zephyr → passthrough tới tool của framework (P1).
#[cfg(feature = "iot")]
fn iot_dev_command(root: &std::path::Path) -> anyhow::Result<(String, Vec<String>)> {
    let adapter = mgc_iot_adapter::adapter_for(root)
        .ok_or_else(|| crate::error::no_framework_detected("IoT", root))?;
    match adapter.framework() {
        "esp32-rust" => Ok(("espflash".to_string(), vec!["monitor".to_string()])),
        "platformio" => Ok(("pio".to_string(), vec!["run".to_string()])),
        "zephyr" => Ok(("west".to_string(), vec!["build".to_string()])),
        other => Err(crate::error::dev_iot_framework_not_implemented(other)),
    }
}

#[cfg(all(test, feature = "game", feature = "iot"))]
#[cfg(test)]
#[path = "../test/dev_test.rs"]
mod tests;
