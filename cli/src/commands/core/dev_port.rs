//! `dev_port.rs` — Bảng port CỐ ĐỊNH cho mọi core (RULE §13: hoán vị 4·3·1·5).
//!
//! ## Quy tắc
//!
//! - Mỗi core có port default cố định, độc nhất trong bảng (không trùng).
//! - `web FE = 4315`, `web BE = 3415` (user chốt trước).
//! - Các core còn lại dùng các hoán vị khác của {1,3,4,5}: 5134/5143/5413/4351/4513/1345.
//! - `--port` flag override 1 lần mà không ghi đè bảng.
//! - Conflict (2 core cùng port): cảnh báo rõ ràng + gợi ý `--port`, KHÔNG crash im lặng.
//!
//! ## Bảng port (chốt 2026-08-19)
//!
//! | Core      | Default Port | Note                         |
//! |-----------|-------------|------------------------------|
//! | web FE    | 4315        | user chốt                    |
//! | web BE    | 3415        | user chốt                    |
//! | ai        | 5134        | hoán vị 4·3·1·5              |
//! | clo       | 5143        | hoán vị 4·3·1·5              |
//! | cicd      | 5413        | hoán vị 4·3·1·5              |
//! | game      | 4351        | hoán vị 4·3·1·5              |
//! | iot       | 4513        | hoán vị 4·3·1·5              |
//! | app       | 1345        | hoán vị 4·3·1·5              |
//! | lib       | 1354        | hoán vị 4·3·1·5 (build-watch)|

use std::net::TcpListener;

/// Port mặc định Web FE (user chốt)
pub const PORT_WEB_FE: u16 = 4315;
/// Port mặc định Web BE (user chốt)
pub const PORT_WEB_BE: u16 = 3415;
/// Port mặc định AI core
pub const PORT_AI: u16 = 5134;
/// Port mặc định Cloud (clo) core
pub const PORT_CLO: u16 = 5143;
/// Port mặc định CI/CD core
pub const PORT_CICD: u16 = 5413;
/// Port mặc định Game core
pub const PORT_GAME: u16 = 4351;
/// Port mặc định IoT core
pub const PORT_IOT: u16 = 4513;
/// Port mặc định App core (mobile/desktop)
pub const PORT_APP: u16 = 1345;
/// Port mặc định Lib core (build-watch server)
pub const PORT_LIB: u16 = 1354;

/// Trả về port mặc định cho một core name (canonical — dùng "clo" không dùng "cloud").
/// Return `None` nếu core không có server (vd: hardware).
pub fn default_port(core: &str) -> Option<u16> {
    match core {
        "web" => Some(PORT_WEB_FE),
        "ai" => Some(PORT_AI),
        "clo" | "cloud" => Some(PORT_CLO),
        "cicd" => Some(PORT_CICD),
        "game" => Some(PORT_GAME),
        "iot" => Some(PORT_IOT),
        "app" => Some(PORT_APP),
        "lib" | "library" => Some(PORT_LIB),
        // hardware không có dev server
        _ => None,
    }
}

/// Kiểm tra port có đang bị occupied không (TCP bind thử).
/// Return `true` nếu port đã bị dùng.
pub fn is_port_in_use(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_err()
}

/// Resolve port cho một core: dùng `--port` override nếu có, không thì dùng bảng default.
/// Cảnh báo nếu port đã bị dùng và gợi ý `--port`.
/// Return port đã chọn (override hoặc default).
pub fn resolve_port(core: &str, port_override: Option<u16>) -> Option<u16> {
    let chosen = port_override.or_else(|| default_port(core))?;
    if is_port_in_use(chosen) {
        mgc_ui::warning(&format!(
            "port {} is already in use for core `{core}`.\n  \
             → Use `mgc dev {core} --port <PORT>` to pick another port.",
            chosen
        ));
    }
    Some(chosen)
}

/// Kiểm tra conflict giữa nhiều core chạy song song.
/// `cores`: danh sách (core_name, port_override) — thứ tự launch.
/// Return danh sách cặp conflict `(core_a, core_b, port)`.
pub fn check_multi_core_conflicts(cores: &[(&str, Option<u16>)]) -> Vec<(String, String, u16)> {
    let mut port_map: std::collections::HashMap<u16, String> = std::collections::HashMap::new();
    let mut conflicts = vec![];
    for (core, override_port) in cores {
        let port = match override_port.or_else(|| default_port(core)) {
            Some(p) => p,
            None => continue,
        };
        if let Some(existing) = port_map.get(&port) {
            conflicts.push((existing.clone(), core.to_string(), port));
        } else {
            port_map.insert(port, core.to_string());
        }
    }
    conflicts
}

// ────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────


#[cfg(test)]
#[path = "../../test/dev_port_test.rs"]
mod tests;
