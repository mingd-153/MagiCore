//! Bevy project scaffolding.

use super::{render_template, TemplateContext};
use mgc_types::MgResult;
use std::path::Path;

/// Scaffold Bevy project với Cargo.toml + main.rs
pub async fn scaffold(context: TemplateContext, target_dir: &Path) -> MgResult<()> {
    let ctx_map = context.to_map();

    // Cargo.toml
    let cargo_toml = render_template(CARGO_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join("Cargo.toml"), cargo_toml)?;

    // src/main.rs
    let src_dir = target_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let main_rs = render_template(MAIN_TEMPLATE, &ctx_map);
    std::fs::write(src_dir.join("main.rs"), main_rs)?;

    // mgc.toml
    let mgc_toml = render_template(MGC_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join("mgc.toml"), mgc_toml)?;

    Ok(())
}

const CARGO_TEMPLATE: &str = r#"[package]
name = "{{project_slug}}"
version = "0.1.0"
edition = "2021"

[dependencies]
bevy = "0.14"

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
"#;

const MAIN_TEMPLATE: &str = r#"use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, hello_world)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn hello_world() {
    println!("Hello from {{project_name}}!");
}
"#;

const MGC_TEMPLATE: &str = r#"name = "{{project_slug}}"
version = "0.1.0"
ecosystem = "game"

[game]
engine = "bevy"

[execution]
architecture = "rust-first"
lane = "native-ready"
"#;


#[cfg(test)]
#[path = "test/bevy_test.rs"]
mod tests;
