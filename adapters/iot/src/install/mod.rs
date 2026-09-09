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

/// ESP32-Rust: cargo orchestrate — toolchain spawn chưa được kiểm chứng E2E,
/// fail-closed thay vì trả danh sách giả.
/// ESP32-Rust: việc spawn toolchain chưa E2E-verified — fail-closed, không
/// trả danh sách cài đặt giả.
async fn install_esp32_rust(project_root: &Path) -> MgResult<Vec<String>> {
    let cargo_toml = project_root.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(MgError::Other("Cargo.toml not found".into()));
    }

    Err(MgError::Unsupported {
        core: "iot",
        capability: "install (esp32-rust)",
        guidance: "esp32-rust install is not E2E-verified yet; \
                   run `cargo fetch` manually until toolchain orchestration lands"
            .to_string(),
    })
}

/// PlatformIO: pio pkg install — chưa chạy tool thật, fail-closed.
/// PlatformIO: chưa chạy `pio pkg install` thật — fail-closed.
async fn install_platformio(project_root: &Path) -> MgResult<Vec<String>> {
    let platformio_ini = project_root.join("platformio.ini");
    if !platformio_ini.exists() {
        return Err(MgError::Other("platformio.ini not found".into()));
    }

    Err(MgError::Unsupported {
        core: "iot",
        capability: "install (platformio)",
        guidance: "PlatformIO install is not implemented yet; \
                   run `pio pkg install` manually"
            .to_string(),
    })
}

/// Zephyr: west update — chưa chạy tool thật, fail-closed.
/// Zephyr: chưa chạy `west update` thật — fail-closed.
async fn install_zephyr(project_root: &Path) -> MgResult<Vec<String>> {
    let west_yml = project_root.join("west.yml");
    if !west_yml.exists() {
        return Err(MgError::Other("west.yml not found".into()));
    }

    Err(MgError::Unsupported {
        core: "iot",
        capability: "install (zephyr)",
        guidance: "Zephyr install is not implemented yet; \
                   run `west update` manually"
            .to_string(),
    })
}

#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
