fn template_layer_ready(rel: &str) -> bool {
    let root = crate::scaffold::template_root::TemplateRoot::resolve(rel);
    root.exists("template.toml") && root.exists("sources")
}

#[test]
fn scaffold_materializes_framework_structure() {
    if !template_layer_ready("game/bevy") {
        eprintln!("skipped: game/bevy template layer not available offline (registry-first)");
        return;
    }
    for fw in ["bevy", "godot", "unity", "unreal"] {
        let dir = std::env::temp_dir().join(format!(
            "mg-game-scaffold-{fw}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let config = crate::wizard::engine::ScaffoldConfig {
            core: "game".to_string(),
            sub_type: String::new(),
            frameworks: vec![fw.to_string()],
            project_name: "game-demo".to_string(),
            features: vec![],
            template_dir: std::path::PathBuf::new(),
        };
        let res = crate::scaffold::processor::Scaffolder::scaffold(&config);
        std::env::set_current_dir(&prev).unwrap();
        res.expect("scaffold game");
        let base = dir.join("game-demo");
        assert!(base.join("mg.toml").exists(), "{fw}: mg.toml missing");

        let expected: &[&str] = match fw {
            "bevy" => &[
                "Cargo.toml",
                "src/main.rs",
                "src/components.rs",
                "src/game_state.rs",
                "src/systems/mod.rs",
                "src/systems/setup.rs",
                "src/systems/movement.rs",
                "assets/README.md",
            ],
            "godot" => &[
                "project.godot",
                "scenes/Main.tscn",
                "scripts/Main.gd",
                "scripts/Player.gd",
                "assets/README.md",
            ],
            "unity" => &["Packages/manifest.json", "Assets/Scripts/Bootstrap.cs"],
            "unreal" => &[
                "game-demo.uproject",
                "Config/DefaultEngine.ini",
                "Source/game-demo/game-demo.Build.cs",
                "Source/game-demo/game-demo.cpp",
                "Source/game-demoEditor.Target.cs",
                "Source/game-demoEditor.Target.cs",
                "Content/README.md",
            ],
            _ => unreachable!(),
        };
        for rel in expected {
            assert!(
                base.join(rel).exists(),
                "{fw}: expected file '{rel}' not materialized"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
