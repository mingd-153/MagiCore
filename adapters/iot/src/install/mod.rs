//! IoT dependency installation per framework.

use crate::framework::IotFramework;
use mgc_types::{MgError, MgResult};
use std::path::Path;

/// Install dependencies for IoT project
pub async fn install_dependencies(
    framework: IotFramework,
    project_root: &Path,
) -> MgResult<Vec<String>> {
    match framework {
        IotFramework::Esp32Rust => install_esp32_rust(project_root).await,
        IotFramework::Platformio => install_platformio(project_root).await,
        IotFramework::Zephyr => install_zephyr(project_root).await,
    }
}

/// ESP32-Rust: cargo orchestrate
async fn install_esp32_rust(project_root: &Path) -> MgResult<Vec<String>> {
    let cargo_toml = project_root.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(MgError::Other("Cargo.toml not found".into()));
    }

    // Stub: cargo fetch
    Ok(vec!["esp-hal@0.17.0".to_string()])
}

/// PlatformIO: pio pkg install
async fn install_platformio(project_root: &Path) -> MgResult<Vec<String>> {
    let platformio_ini = project_root.join("platformio.ini");
    if !platformio_ini.exists() {
        return Err(MgError::Other("platformio.ini not found".into()));
    }

    // Stub: pio pkg install
    Ok(vec![])
}

/// Zephyr: west update
async fn install_zephyr(project_root: &Path) -> MgResult<Vec<String>> {
    let west_yml = project_root.join("west.yml");
    if !west_yml.exists() {
        return Err(MgError::Other("west.yml not found".into()));
    }

    // Stub: west update
    Ok(vec![])
}


#[cfg(test)]
#[path = "../test/mod_test.rs"]
mod tests;
