//! IoT project scaffolding.

use crate::framework::{IotFramework, board_target_for_framework, boards_for_framework};
use mgc_types::MgResult;
use std::path::Path;

/// Scaffold an IoT project only for a board registered to the selected framework.
/// Chỉ scaffold project IoT khi board được đăng ký cho framework đã chọn.
pub async fn scaffold_project(
    framework: IotFramework,
    project_name: &str,
    board: &str,
    target_dir: &Path,
) -> MgResult<()> {
    if board_target_for_framework(framework.as_str(), board).is_none() {
        let supported = boards_for_framework(framework.as_str())
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(mgc_types::MgError::Unsupported {
            core: "iot",
            capability: "scaffold_board",
            guidance: format!(
                "board '{board}' is not registered for framework '{}'; supported boards: {supported}",
                framework.as_str()
            ),
        });
    }

    std::fs::create_dir_all(target_dir)?;

    match framework {
        IotFramework::Esp32Rust => scaffold_esp32(project_name, board, target_dir).await,
        IotFramework::Platformio => scaffold_platformio(project_name, board, target_dir).await,
        IotFramework::Zephyr => scaffold_zephyr(project_name, board, target_dir).await,
    }
}

async fn scaffold_esp32(name: &str, board: &str, dir: &Path) -> MgResult<()> {
    let cargo = format!(
        "[package]\nname=\"{}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\nesp-hal=\"1.2\"\n",
        name
    );
    std::fs::write(dir.join("Cargo.toml"), cargo)?;

    let mgc = format!(
        "name=\"{}\"\nversion=\"0.1.0\"\necosystem=\"iot\"\n\n[iot]\nframework=\"esp32-rust\"\nboard=\"{}\"\n",
        name, board
    );
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

    let mgc = format!(
        "name=\"{}\"\nversion=\"0.1.0\"\necosystem=\"iot\"\n\n[iot]\nframework=\"platformio\"\nboard=\"{}\"\n",
        name, board
    );
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

    let mgc = format!(
        "name=\"{}\"\nversion=\"0.1.0\"\necosystem=\"iot\"\n\n[iot]\nframework=\"zephyr\"\nboard=\"{}\"\n",
        name, board
    );
    std::fs::write(dir.join("mgc.toml"), mgc)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
