//! Tests for `mgc init` project metadata generation.
//! Kiểm tra metadata do `mgc init` tạo theo đúng framework được chọn.

use super::{ScaffoldConfig, write_mgc_toml};
use std::path::PathBuf;

#[test]
fn game_init_only_adds_cargo_scripts_for_bevy() {
    for framework in ["bevy", "godot", "unity", "unreal"] {
        let project = tempfile::tempdir().expect("temporary project directory");
        let config = ScaffoldConfig {
            core: "game".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework.to_string()],
            project_name: format!("test-{framework}"),
            features: vec![],
            template_dir: PathBuf::new(),
        };

        write_mgc_toml(project.path(), &config).expect("write project metadata");

        let generated = std::fs::read_to_string(project.path().join("mgc.toml"))
            .expect("read generated project metadata");
        if framework == "bevy" {
            assert!(generated.contains("run = \"cargo run\""));
            assert!(generated.contains("build = \"cargo build\""));
        } else {
            assert!(
                !generated.contains("cargo run") && !generated.contains("cargo build"),
                "{framework} must not receive Rust/Cargo lifecycle scripts"
            );
        }
    }
}
