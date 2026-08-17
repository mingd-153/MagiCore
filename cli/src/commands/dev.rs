use anyhow::bail;
use mg_ui::info;

use crate::context::ProjectContext;

/// Lệnh dev cho từng engine game (Q15/Q20): bevy → cargo run; godot → mở editor.
fn game_dev_command(root: &std::path::Path) -> anyhow::Result<(String, Vec<String>)> {
    let adapter = mg_game_adapter::adapter_for(root)
        .ok_or_else(|| anyhow::anyhow!("No game engine detected in {}", root.display()))?;
    match adapter.engine() {
        "bevy" => Ok(("cargo".to_string(), vec!["run".to_string()])),
        "godot" => {
            let path = root
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("project path is not valid UTF-8"))?;
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
            let path = root
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("project path is not valid UTF-8"))?;
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
            Err(anyhow::anyhow!(
                "unreal dev chạy trong Unreal Editor (engine binary đặt tên theo version, không có PATH chuẩn) — mở {uproject} bằng editor, hoặc cài UnrealBuildTool và chạy build trực tiếp. Run `mg build` + editor để chạy."
            ))
        }
        other => Err(anyhow::anyhow!(
            "'mg dev' cho engine '{other}' chưa có lệnh — dùng editor của engine để chạy"
        )),
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
            crate::commands::core::web::dev_at_root(&root, Some(host), port).await
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
                    anyhow::anyhow!(
                        "godot failed: {e} — install the Godot editor first (https://godotengine.org) and ensure `godot` is in PATH"
                    )
                } else {
                    anyhow::anyhow!("{} failed: {e}", cmd)
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
                    anyhow::anyhow!(
                        "espflash failed: {e} — install espflash first (`cargo install espflash`) and ensure it is in PATH"
                    )
                } else {
                    anyhow::anyhow!("{} failed: {e}", cmd)
                }
            })?;
            Ok(())
        }
        "cloud" => {
            crate::commands::core::clo::dev(false).await?;
            Ok(())
        }
        "cicd" => {
            crate::commands::core::cicd::dev(false).await?;
            Ok(())
        }
        "app" => {
            crate::commands::core::app::dev(false).await?;
            Ok(())
        }
        "ai" => {
            crate::commands::core::ai::dev(false).await?;
            Ok(())
        }
        other => bail!("'mg dev' Engine is not implemented for the '{other}' core yet"),
    }
}

/// Lệnh dev cho từng framework IoT (Q16/Q20): esp32-rust → espflash monitor;
/// platformio/zephyr → passthrough tới tool của framework (P1).
fn iot_dev_command(root: &std::path::Path) -> anyhow::Result<(String, Vec<String>)> {
    let adapter = mg_iot_adapter::adapter_for(root)
        .ok_or_else(|| anyhow::anyhow!("No IoT framework detected in {}", root.display()))?;
    match adapter.framework() {
        "esp32-rust" => Ok(("espflash".to_string(), vec!["monitor".to_string()])),
        "platformio" => Ok(("pio".to_string(), vec!["run".to_string()])),
        "zephyr" => Ok(("west".to_string(), vec!["build".to_string()])),
        other => bail!("'mg dev' for '{other}' iot framework is not implemented yet"),
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
