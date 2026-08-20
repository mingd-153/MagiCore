use mg_ui::info;

use crate::context::ProjectContext;

/// Lệnh dev cho từng engine game (Q15/Q20): bevy → cargo run; godot → mở editor.
fn game_dev_command(root: &std::path::Path) -> anyhow::Result<(String, Vec<String>)> {
    let adapter = mg_game_adapter::adapter_for(root)
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

    info("Starting MegaGate Native Dev Server...");
    info(&format!("Project root: {}", root.display()));
    info(&format!("Execution profile: {}", ctx.execution_summary()));

    match ctx.adapter().name() {
        "web" => {
            if clear {
                info("--clear is delegated to the selected web framework when supported.");
            }
            crate::commands::core::dev::web::dev_at_root(&root, Some(host), port).await
        }
        "game" => {
            let (cmd, args) = game_dev_command(&root)?;
            let opts = mg_exec::prelude::ExecOptions {
                cwd: Some(root.clone()),
                log_path: Some(root.join(".megagate").join("exec.log")),
                clean_env: true,
                ..Default::default()
            };
            info(&format!(
                "Game dev: running `{} {}`...",
                cmd,
                args.join(" ")
            ));
            mg_exec::prelude::run_inherited(&cmd, &args, &opts).map_err(|e| {
                if cmd == "godot" {
                    crate::error::godot_failed(&e)
                } else {
                    crate::error::tool_failed(&cmd, &e)
                }
            })?;
            Ok(())
        }
        "iot" => {
            let (cmd, args) = iot_dev_command(&root)?;
            let opts = mg_exec::prelude::ExecOptions {
                cwd: Some(root.clone()),
                log_path: Some(root.join(".megagate").join("exec.log")),
                clean_env: true,
                ..Default::default()
            };
            info(&format!("IoT dev: running `{} {}`...", cmd, args.join(" ")));
            mg_exec::prelude::run_inherited(&cmd, &args, &opts).map_err(|e| {
                if cmd == "espflash" {
                    crate::error::espflash_failed(&e)
                } else {
                    crate::error::tool_failed(&cmd, &e)
                }
            })?;
            Ok(())
        }
        "cloud" => {
            crate::commands::core::dev::clo::dev(false).await?;
            Ok(())
        }
        "cicd" => {
            crate::commands::core::dev::cicd::dev(false).await?;
            Ok(())
        }
        "app" => {
            crate::commands::core::dev::app::dev(false).await?;
            Ok(())
        }
        "ai" => {
            crate::commands::core::dev::ai::dev(false).await?;
            Ok(())
        }
        other => Err(crate::error::dev_core_not_implemented(other)),
    }
}

/// Lệnh dev cho từng framework IoT (Q16/Q20): esp32-rust → espflash monitor;
/// platformio/zephyr → passthrough tới tool của framework (P1).
fn iot_dev_command(root: &std::path::Path) -> anyhow::Result<(String, Vec<String>)> {
    let adapter = mg_iot_adapter::adapter_for(root)
        .ok_or_else(|| crate::error::no_framework_detected("IoT", root))?;
    match adapter.framework() {
        "esp32-rust" => Ok(("espflash".to_string(), vec!["monitor".to_string()])),
        "platformio" => Ok(("pio".to_string(), vec!["run".to_string()])),
        "zephyr" => Ok(("west".to_string(), vec!["build".to_string()])),
        other => Err(crate::error::dev_iot_framework_not_implemented(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_dev_bevy_runs_cargo() {
        let dir = std::env::temp_dir().join(format!("mg-dev-bevy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[package.metadata.megagate]\ncore = \"game\"\n\n[dependencies]\n",
        )
        .unwrap();
        let (cmd, args) = game_dev_command(&dir).unwrap();
        assert_eq!(cmd, "cargo");
        assert_eq!(args, vec!["run"]);
    }

    #[test]
    fn game_dev_godot_opens_editor() {
        let dir = std::env::temp_dir().join(format!("mg-dev-godot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("project.godot"), "[application]\nname=\"demo\"\n").unwrap();
        let (cmd, args) = game_dev_command(&dir).unwrap();
        assert_eq!(cmd, "godot");
        assert!(args.iter().any(|a| a == "--editor"));
        assert!(args.iter().any(|a| a == "--path"));
    }

    #[test]
    fn game_dev_unity_opens_editor_cli() {
        let dir = std::env::temp_dir().join(format!("mg-dev-unity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Packages")).unwrap();
        std::fs::write(dir.join("Packages").join("manifest.json"), "{}").unwrap();
        let (cmd, args) = game_dev_command(&dir).unwrap();
        assert_eq!(cmd, "unity");
        assert!(args.iter().any(|a| a == "-projectPath"));
    }

    #[test]
    fn game_dev_unreal_hints_editor() {
        let dir = std::env::temp_dir().join(format!("mg-dev-unreal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Game.uproject"), "{}").unwrap();
        let err = game_dev_command(&dir).unwrap_err();
        assert!(err.to_string().contains("Game.uproject"));
    }

    #[test]
    fn game_dev_unknown_engine_bails() {
        assert!(game_dev_command(std::path::Path::new("/nonexistent")).is_err());
    }

    #[test]
    fn iot_dev_esp32_uses_espflash_monitor() {
        let dir = std::env::temp_dir().join(format!("mg-dev-iot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        let (cmd, args) = iot_dev_command(&dir).unwrap();
        assert_eq!(cmd, "espflash");
        assert_eq!(args, vec!["monitor"]);
    }

    #[test]
    fn iot_dev_platformio_uses_pio() {
        let dir = std::env::temp_dir().join(format!("mg-dev-pio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("platformio.ini"), "[env:esp32dev]\n").unwrap();
        let (cmd, args) = iot_dev_command(&dir).unwrap();
        assert_eq!(cmd, "pio");
        assert_eq!(args, vec!["run"]);
    }

    #[test]
    fn iot_dev_unknown_framework_bails() {
        assert!(iot_dev_command(std::path::Path::new("/nonexistent")).is_err());
    }
}
