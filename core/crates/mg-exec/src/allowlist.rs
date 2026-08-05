//! Allowlist check — kiểm tra tool trước khi exec (00-index §5.1, §5.2)
//! (Exec passthrough allowlist: 00-index §5.1 allowlist bất biến + §5.2 cấm vĩnh viễn)

use anyhow::{bail, Result};

/// Tools được phép passthrough — allowlist bất biến (00-index §5.1).
/// Mỗi core khai báo subset; thêm tool phải review + ghi lý do.
pub const ALLOWED_TOOLS: &[&str] = &[
    "pip", "uv", "pub", "dart", "gradle", "swift", "cargo", "espflash", "west", "pio",
    "platformio", "terraform", "tofu", "aws", "wrangler", "gcloud", "gh", "docker", "godot",
    "flutter", "kotlinc", "python", "unity", "upm",
];

/// Tools cấm vĩnh viễn — format có resolver mg (00-index §5.2) nên wrapper bị cấm.
/// (npm/npx/pnpm/yarn/bun cấm mọi core — gọi mg install thay vì npm)
pub const FORBIDDEN_TOOLS: &[&str] = &["npm", "npx", "pnpm", "yarn", "bun"];

/// Kiểm tool trước khi exec: cấm vĩnh viễn → lỗi rõ lý do; ngoài allowlist → lỗi.
pub fn check_tool(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("tool name is empty");
    }
    if FORBIDDEN_TOOLS.contains(&name) {
        bail!(
            "tool '{name}' is permanently forbidden (mg resolver covers its format — use `mg install` instead)"
        );
    }
    if !ALLOWED_TOOLS.contains(&name) {
        bail!(
            "tool '{name}' is not on the allowlist (00-index §5.1) — add it there only after review"
        );
    }
    Ok(())
}