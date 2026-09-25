// Registry configuration for core-web — validates effective registry endpoints.
// Cấu hình registry của core-web — gom policy endpoint khỏi adapter chính.
//
// P0-4 (Tech Lead 2026-09-15): the guards below used to `panic!` on an
// invalid or unallowed registry URL. Panics are not a typed failure mode:
// any caller (CLI dispatch, tests, benches) could abort the whole process
// with no chance to report or recover. Both guards now return
// `Result` and stay FAIL-CLOSED — an invalid URL is still REJECTED,
// only the rejection is a typed error instead of an abort.
// (P0-4: các guard dưới đây từng `panic!` khi URL registry không hợp lệ
// hoặc không nằm trong danh sách cho phép. Panic không phải chế độ lỗi
// typed: bất kỳ caller nào cũng có thể làm sập cả process. Cả hai guard
// giờ trả `Result` và vẫn FAIL-CLOSED — URL không hợp lệ vẫn bị CHẶN,
// chỉ khác là chặn bằng typed error thay vì abort.)

use crate::audit::allow_insecure_loopback_url;
use anyhow::Result;

pub const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";

pub fn effective_registry_url(default: &str) -> Result<String> {
    let url = std::env::var("MAGICORE_WEB_REGISTRY_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string());
    if !url.starts_with("https://") && !allow_insecure_loopback_url(&url) {
        return Err(anyhow::anyhow!(
            "registry URL must use HTTPS: '{url}' (loopback http://127.0.0.1/localhost is allowed)"
        ));
    }
    validate_registry_allowed(&url)?;
    Ok(url)
}

pub fn validate_registry_allowed(url: &str) -> Result<()> {
    let Some(allowed) = std::env::var("MAGICORE_WEB_ALLOWED_REGISTRIES").ok() else {
        return Ok(());
    };
    let allowed_list: Vec<&str> = allowed
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if allowed_list.is_empty() {
        return Ok(());
    }
    let normalized = url.trim_end_matches('/');
    let matched = allowed_list
        .iter()
        .any(|a| normalized == a.trim_end_matches('/'));
    if matched {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "registry '{}' is not in MAGICORE_WEB_ALLOWED_REGISTRIES ({})",
        url,
        allowed
    ))
}
