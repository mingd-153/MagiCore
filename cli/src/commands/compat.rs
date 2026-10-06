//! `compat.rs` — Native-only rival-runtime refusal gate
//! architecture, Tech Lead 2026-09-10).
//!
//! MagiCore's execution path is native; rival JS runtimes are never used
//! as MagiCore's execution engine.
//! Historical compatibility flags are parsed for a precise refusal, but
//! cannot delegate execution to Bun or Deno.
//!
//! Runtime JS đối thủ không được dùng làm engine MagiCore. Cờ tương thích
//! cũ chỉ được parse để trả lỗi rõ; không mở đường delegate Bun/Deno.

use anyhow::{Result, bail};

/// Historical rival-runtime flag values accepted for actionable refusal.
/// Các giá trị cờ runtime cũ được nhận diện để từ chối có hướng dẫn.
pub const COMPAT_RUNTIMES: &[&str] = &["bun", "deno"];

/// External package managers that must NEVER be spawned by the native
/// path regardless of mode (mgc resolver owns installs).
/// PM ngoài không bao giờ được spawn trên đường native bất kể chế độ
/// (resolver của mgc sở trợ install).
pub const FORBIDDEN_RUNTIMES: &[&str] = &["npm", "npx", "pnpm", "yarn", "bunx"];

/// Resolved compatibility mode for one command invocation.
/// Chế độ compat đã resolve cho một lần gọi lệnh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatMode {
    /// Native MagiCore engine — the default, no rival runtime spawns.
    /// Engine native MagiCore — mặc định, không spawn runtime đối thủ.
    Native,
    /// Historical explicit opt-in, retained only to report a clear refusal.
    /// Cờ opt-in lịch sử, chỉ giữ để trả lời từ chối rõ ràng.
    Explicit(String),
}

impl CompatMode {
    /// Parse the legacy flag to reject unknown values consistently.
    /// Parse cờ cũ để từ chối giá trị không nhận diện một cách nhất quán.
    pub fn from_flag(flag: Option<&str>) -> Result<Self> {
        let value = match flag {
            Some(v) => Some(v.to_string()),
            None => std::env::var("MGC_COMPAT_RUNTIME").ok(),
        };
        let Some(value) = value else {
            return Ok(Self::Native);
        };
        let lowered = value.to_ascii_lowercase();
        if !COMPAT_RUNTIMES.contains(&lowered.as_str()) {
            bail!(
                "invalid --compat-runtime '{value}' — recognized legacy values: {}; execution through rival runtimes is unsupported",
                COMPAT_RUNTIMES.join(", ")
            );
        }
        // Package managers are NEVER spawn-able, even in compat mode —
        // installs belong to the mgc resolver.
        // PM không bao giờ spawn được, kể cả trong compat — install là
        // việc của resolver mgc.
        Ok(Self::Explicit(lowered))
    }
}

/// Refuse rival runtimes in every mode; compatibility flags cannot
/// delegate execution outside the native engine.
/// Từ chối runtime đối thủ ở mọi mode; cờ tương thích không delegate.
pub fn gate_runtime_spawn(_mode: &CompatMode, runtime: &str) -> Result<()> {
    let lowered = runtime.to_ascii_lowercase();
    if FORBIDDEN_RUNTIMES.contains(&lowered.as_str()) {
        bail!(
            "'{runtime}' is never spawnable by mgc — installs belong to the native resolver (`mgc install`)"
        );
    }
    if !COMPAT_RUNTIMES.contains(&lowered.as_str()) {
        // Not a compat runtime at all — handled elsewhere (toolchain
        // binaries like cargo/go are allowlisted in mgc-exec).
        // Không phải runtime compat — xử lý chỗ khác (binary toolchain
        // như cargo/go nằm trong allowlist mgc-exec).
        return Ok(());
    }
    Err(crate::error::rival_runtime_not_native(&lowered))
}
