//! Game scaffold: unity/godot/unreal/bevy templates.

use std::path::Path;

use anyhow::Result;

use super::{write_file, slugify};

pub struct GameProcessor;

impl GameProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "unity" => {
                write_file(
                    &target.join("Packages").join("manifest.json"),
                    "{\n  \"dependencies\": {}\n}\n",
                )?;
                write_file(
                    &target.join("Assets").join("Scripts").join("Bootstrap.cs"),
                    "using UnityEngine;\n\npublic class Bootstrap : MonoBehaviour\n{\n    void Start()\n    {\n        Debug.Log(\"MegaGate Unity project ready\");\n    }\n}\n",
                )?;
            }
            "godot" => {
                write_file(
                    &target.join("project.godot"),
                    "[application]\nconfig/name=\"MegaGate Game\"\nrun/main_scene=\"res://Main.tscn\"\n",
                )?;
                write_file(
                    &target.join("Main.tscn"),
                    "[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node2D\"]\n",
                )?;
            }
            "unreal" => {
                write_file(
                    &target.join(format!("{name}.uproject")),
                    &format!(
                        "{{\n  \"FileVersion\": 3,\n  \"EngineAssociation\": \"5.0\",\n  \"Category\": \"Games\",\n  \"Description\": \"{}\"\n}}\n",
                        name
                    ),
                )?;
            }
            _ => {
                write_file(
                    &target.join("Cargo.toml"),
                    &format!(
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbevy = \"0.14\"\n",
                        slugify(name)
                    ),
                )?;
                write_file(
                    &target.join("src").join("main.rs"),
                    "use bevy::prelude::*;\n\nfn main() {\n    App::new()\n        .add_plugins(DefaultPlugins)\n        .add_systems(Startup, setup)\n        .add_systems(Update, frame_counter)\n        .run();\n}\n\nfn setup(mut commands: Commands) {\n    commands.spawn(Camera2d);\n    commands.spawn(Sprite {\n        color: Color::srgb(0.2, 0.8, 0.4),\n        custom_size: Some(Vec2::new(100.0, 100.0)),\n        ..default()\n    });\n}\n\nfn frame_counter(mut frames: Local<u64>) {\n    *frames += 1;\n    if *frames % 60 == 0 {\n        println!(\"MegaGate bevy game running ({} frames)\", *frames);\n    }\n}\n",
                )?;
            }
        }

        Ok(())
    }

}
