//! IoT project scaffolding.

use crate::framework::IotFramework;
use mgc_types::MgResult;
use std::path::Path;

/// Scaffold IoT project
pub async fn scaffold_project(
    framework: IotFramework,
    project_name: &str,
    board: &str,
    target_dir: &Path,
) -> MgResult<()> {
    std::fs::create_dir_all(target_dir)?;

    match framework {
        IotFramework::Esp32Rust => scaffold_esp32(project_name, board, target_dir).await,
        IotFramework::Platformio => scaffold_platformio(project_name, board, target_dir).await,
        IotFramework::Zephyr => scaffold_zephyr(project_name, board, target_dir).await,
    }
}

async fn scaffold_esp32(name: &str, board: &str, dir: &Path) -> MgResult<()> {
    let cargo = format!("[package]\nname=\"{}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\nesp-hal=\"0.17\"\n", name);
    std::fs::write(dir.join("Cargo.toml"), cargo)?;

    let mgc = format!("name=\"{}\"\nversion=\"0.1.0\"\necosystem=\"iot\"\n\n[iot]\nframework=\"esp32-rust\"\nboard=\"{}\"\n", name, board);
    std::fs::write(dir.join("mgc.toml"), mgc)?;

    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::write(
        dir.join("src/main.rs"),
        "#![no_std]\n#![no_main]\n\nfn main() {}\n",
    )?;
    Ok(())
}

async fn scaffold_platformio(name: &str, board: &str, dir: &Path) -> MgResult<()> {
    let ini = format!(
        "[env:{}]\nplatform=espressif32\nboard={}\nframework=arduino\n",
        board, board
    );
    std::fs::write(dir.join("platformio.ini"), ini)?;

    let mgc = format!("name=\"{}\"\nversion=\"0.1.0\"\necosystem=\"iot\"\n\n[iot]\nframework=\"platformio\"\nboard=\"{}\"\n", name, board);
    std::fs::write(dir.join("mgc.toml"), mgc)?;

    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::write(
        dir.join("src/main.cpp"),
        "void setup() {}\nvoid loop() {}\n",
    )?;
    Ok(())
}

async fn scaffold_zephyr(name: &str, board: &str, dir: &Path) -> MgResult<()> {
    let west = "manifest:\n  projects:\n    - name: zephyr\n      url: https://github.com/zephyrproject-rtos/zephyr\n"
        .to_string();
    std::fs::write(dir.join("west.yml"), west)?;

    let mgc = format!("name=\"{}\"\nversion=\"0.1.0\"\necosystem=\"iot\"\n\n[iot]\nframework=\"zephyr\"\nboard=\"{}\"\n", name, board);
    std::fs::write(dir.join("mgc.toml"), mgc)?;
    Ok(())
}


#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
