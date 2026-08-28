#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for dev server command

use super::*;

    use super::*;

    #[test]
    fn game_dev_bevy_runs_cargo() {
        let dir = std::env::temp_dir().join(format!("mgc-dev-bevy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"game\"\n\n[dependencies]\n",
        )
        .unwrap();
        let (cmd, args) = game_dev_command(&dir).unwrap();
        assert_eq!(cmd, "cargo");
        assert_eq!(args, vec!["run"]);
    }

    #[test]
    fn game_dev_godot_opens_editor() {
        let dir = std::env::temp_dir().join(format!("mgc-dev-godot-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("mgc-dev-unity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Packages")).unwrap();
        std::fs::write(dir.join("Packages").join("manifest.json"), "{}").unwrap();
        let (cmd, args) = game_dev_command(&dir).unwrap();
        assert_eq!(cmd, "unity");
        assert!(args.iter().any(|a| a == "-projectPath"));
    }

    #[test]
    fn game_dev_unreal_hints_editor() {
        let dir = std::env::temp_dir().join(format!("mgc-dev-unreal-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("mgc-dev-iot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        let (cmd, args) = iot_dev_command(&dir).unwrap();
        assert_eq!(cmd, "espflash");
        assert_eq!(args, vec!["monitor"]);
    }

    #[test]
    fn iot_dev_platformio_uses_pio() {
        let dir = std::env::temp_dir().join(format!("mgc-dev-pio-{}", std::process::id()));
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
