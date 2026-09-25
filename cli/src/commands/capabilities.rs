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

/// Dependency ownership per core, derived from the C0 firewall table
/// (`dep_gate::owner_for` over full `DepContext`s) — the SINGLE
/// machine-readable source the lifecycle matrix cross-checks against
/// (T0.4). Shape per operation: {"owner", "tools"}; cores with
/// per-ecosystem splits additionally carry a "languages" map with ONLY
/// the differing ecosystems (gate ids: "rn", not "react-native").
/// Quyền sở hữu dependency từng core, suy ra từ bảng tường lửa C0 —
/// nguồn máy-đọc DUY NHẤT matrix cross-check (T0.4).
pub fn dependency_ownership(core: &str) -> serde_json::Value {
    use crate::commands::dep_gate::{DepContext, DepOwner, SPLIT_LANGUAGES, owner_for};
    fn ops_for(core: &str, ecosystem: Option<&str>) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for op in crate::commands::dep_gate::DepOp::ALL {
            let ctx = DepContext::new(core, ecosystem, None, None, *op);
            let (owner, tools): (&str, Vec<String>) = match owner_for(&ctx) {
                DepOwner::Native => ("mgc-native", Vec::new()),
                DepOwner::ScaffoldOnly => ("scaffold-only", Vec::new()),
                DepOwner::Unsupported => ("unsupported", Vec::new()),
            };
            map.insert(
                op.as_str().to_string(),
                serde_json::json!({"owner": owner, "tools": tools}),
            );
        }
        serde_json::Value::Object(map)
    }
    let base = ops_for(core, None);
    let mut languages = serde_json::Map::new();
    for language in SPLIT_LANGUAGES {
        let split = ops_for(core, Some(language));
        if split != base {
            languages.insert((*language).to_string(), split);
        }
    }
    let mut root = serde_json::Map::new();
    root.insert("operations".to_string(), base);
    if !languages.is_empty() {
        root.insert(
            "languages".to_string(),
            serde_json::Value::Object(languages),
        );
    }
    serde_json::Value::Object(root)
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
                "dependency_ownership": dependency_ownership(name),
                "framework_qualification": crate::commands::framework_records::qualification_json(name),
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
                    "dependency_ownership": dependency_ownership(name),
                    "framework_qualification": crate::commands::framework_records::qualification_json(name),
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
