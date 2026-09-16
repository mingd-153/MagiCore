//! `mgc capabilities` — machine-readable capability manifest (Global Gate 1,
//! Tech Lead 2026-09-16). The hand-written matrix table is replaced by the
//! binary's own declaration: each adapter carries a `CAPABILITIES` const and
//! this command prints it. Anything not listed is `unsupported` — the
//! negative gate (`mgc-types/tests/capabilities.rs`) enforces honesty.
//! `mgc capabilities` — bảng capability máy-đọc (Global Gate 1). Bảng matrix
//! viết tay được thay bằng tuyên bố của chính binary: mỗi adapter mang const
//! `CAPABILITIES` và lệnh này in nó. Cái không liệt kê là `unsupported` —
//! gate âm tính trong `mgc-types/tests/capabilities.rs` đảm bảo tính trung
//! thực.

use anyhow::Result;
use mgc_types::capabilities::Capability;

/// CLI-canonical core names (matrix-lane naming: cloud = "clo").
/// Tên core chuẩn của CLI (đặt tên theo lane matrix: cloud = "clo").
const ALL_CORES: [&str; 9] = [
    "web", "lib", "ai", "app", "game", "iot", "clo", "cicd", "hardware",
];

/// Static capability manifest per core — no adapter construction needed
/// (detection is not required to READ the claims).
/// Bảng capability tĩnh theo core — không cần dựng adapter (đọc claim không
/// cần detect project).
fn capabilities_for_core(core: &str) -> Result<&'static [Capability]> {
    let caps: &'static [Capability] = match core {
        #[cfg(feature = "web")]
        "web" => mgc_web_adapter::WebAdapter::CAPABILITIES,
        #[cfg(feature = "lib")]
        "lib" => mgc_lib_adapter::LibAdapter::CAPABILITIES,
        #[cfg(feature = "ai")]
        "ai" => mgc_ai_adapter::AiAdapter::CAPABILITIES,
        #[cfg(feature = "app")]
        "app" => mgc_app_adapter::AppAdapter::CAPABILITIES,
        #[cfg(feature = "game")]
        "game" => mgc_game_adapter::GameAdapter::CAPABILITIES,
        #[cfg(feature = "iot")]
        "iot" => mgc_iot_adapter::IotAdapter::CAPABILITIES,
        #[cfg(feature = "clo")]
        "clo" => mgc_cloud_adapter::CloudAdapter::CAPABILITIES,
        #[cfg(feature = "cicd")]
        "cicd" => mgc_cicd_adapter::CicdAdapter::CAPABILITIES,
        #[cfg(feature = "hardware")]
        "hardware" => mgc_hardware_adapter::HardwareAdapter::CAPABILITIES,
        other => return Err(crate::error::unknown_core(other)),
    };
    Ok(caps)
}

/// `mgc capabilities [--core <core>]` — with `--core`, prints the single
/// chotted shape; without it, prints all built cores (the lifecycle matrix
/// consumes the all-cores shape).
/// `mgc capabilities [--core <core>]` — có `--core`, in shape đơn đã chốt;
/// không có, in mọi core đã build (lifecycle matrix dùng shape all-cores).
pub fn run(core: Option<&str>) -> Result<()> {
    match core {
        Some(name) => {
            let caps = capabilities_for_core(name)?;
            let payload = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "core": name,
                "capabilities": caps,
                "everything_else": "unsupported",
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        None => {
            let mut cores = Vec::new();
            for name in ALL_CORES {
                // Cores not compiled into this binary are skipped — the
                // matrix gate treats an absent row as fail-closed.
                // Core không được compile vào binary bị bỏ qua — gate matrix
                // coi hàng vắng mặt là fail-closed.
                let caps = match capabilities_for_core(name) {
                    Ok(caps) => caps,
                    Err(_) => continue,
                };
                cores.push(serde_json::json!({
                    "core": name,
                    "capabilities": caps,
                    "everything_else": "unsupported",
                }));
            }
            let payload = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "cores": cores,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
    }
    Ok(())
}
