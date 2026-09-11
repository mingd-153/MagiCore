//! `compat.rs` — Explicit compatibility-runtime gate (native-engine
//! architecture, Tech Lead 2026-09-10).
//!
//! MagiCore's default execution path is NATIVE: MgDevServer, the
//! bundler, resolver, and the process runner — NOT Bun/Deno. A rival
//! JS runtime may only ever run in COMPATIBILITY mode: an explicit
//! `--compat-runtime` flag (or `MGC_COMPAT_RUNTIME` env), with a loud
//! warning on every spawn. No auto-detection, no silent forwarding.
//!
//! Contract (per the architecture ruling):
//!   mgc dev / test / run / build      → native/default MagiCore engine
//!   mgc dev --compat-runtime bun      → temporary compat, explicit + warned
//!   mgc import bun                    → migration source (lockfile ingest)
//!   mgc benchmark --against bun       → comparison target
//!
//! Runtime JS tương thích chỉ được chạy trong chế độ compat: cờ
//! tường minh `--compat-runtime` (hoặc env `MGC_COMPAT_RUNTIME`) kèm
//! cảnh báo lớn mỗi lần spawn. Không auto-detect, không chuyển tiếp
//! âm thầm — đường mặc định của mgc là engine native.

use anyhow::{Result, bail};

/// The runtimes allowed in compatibility mode (rival JS runtimes only —
/// Node.js tooling like `tsc` is NOT a rival runtime lane here).
/// Runtime được phép trong chế độ compat (chỉ runtime đối thủ JS —
/// tooling kiểu `tsc` chạy bằng node không thuộc lane này).
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
    /// Explicit compatibility mode with the named runtime + warning.
    /// Chế độ compat tường minh với runtime nêu tên + cảnh báo.
    Explicit(String),
}

impl CompatMode {
    /// Parse `--compat-runtime` value (None → native). Env var is the
    /// CI-friendly equivalent — both are EXPLICIT opt-ins, never a
    /// file-based auto-detect.
    /// Parse giá trị `--compat-runtime` (None → native). Env là tương
    /// đương thân thiện CI — cả hai đều CHỌN TƯỜNG MINH, không bao giờ
    /// auto-detect theo file.
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
                "invalid --compat-runtime '{value}' — supported: {} (rival JS runtimes in compatibility mode only)",
                COMPAT_RUNTIMES.join(", ")
            );
        }
        // Package managers are NEVER spawn-able, even in compat mode —
        // installs belong to the mgc resolver.
        // PM không bao giờ spawn được, kể cả trong compat — install là
        // việc của resolver mgc.
        Ok(Self::Explicit(lowered))
    }

    /// True when the named runtime is allowed to spawn under this mode.
    /// Đúng khi runtime nêu tên được phép spawn trong chế độ này.
    pub fn allows(&self, runtime: &str) -> bool {
        match self {
            Self::Native => false,
            Self::Explicit(allowed) => allowed == runtime,
        }
    }

    /// The loud warning every compat spawn must print (RULE §11: escape
    /// hatches announce themselves).
    /// Cảnh báo lớn mỗi lần spawn compat phải in (RULE §11: escape
    /// hatch luôn tự báo).
    pub fn warn_once(&self, runtime: &str) {
        if let Self::Explicit(_) = self {
            mgc_ui::warning(&format!(
                "COMPATIBILITY MODE: spawning rival runtime '{runtime}' — this is NOT the native MagiCore engine path. Use `mgc import {runtime}` to migrate the project, or `mgc benchmark --against {runtime}` to compare."
            ));
        }
    }
}

/// Gate a rival-runtime spawn decision: Native mode and forbidden
/// runtimes fail with a migration-pointing error; compat mode warns and
/// allows only the opted-in runtime.
/// Chặn quyết định spawn runtime đối thủ: Native và runtime bị cấm fail
/// kèm lỗi trỏ về migration; compat cảnh báo và chỉ cho runtime đã chọn.
pub fn gate_runtime_spawn(mode: &CompatMode, runtime: &str) -> Result<()> {
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
    if !mode.allows(&lowered) {
        bail!(
            "refusing to spawn '{runtime}': mgc dev/test/run/build execute on the NATIVE MagiCore engine. To run a Bun/Deno project temporarily, pass --compat-runtime {runtime} (explicit, warned) — or migrate with `mgc import {runtime}`"
        );
    }
    mode.warn_once(&lowered);
    Ok(())
}
