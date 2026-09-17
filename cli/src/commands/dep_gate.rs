//! `dep_gate.rs` — C0 ownership firewall: the single control path for
//! dependency-lifecycle operations (T0.3, V1.2 Native Ownership Contract).
//!
//! Every dependency lane (install/add/remove/update/list) MUST pass through
//! `gate()` before spawning a toolchain or routing to an adapter:
//! native cells proceed, delegated cells require an explicit
//! `--compat-runtime` opt-in (warned + audit-logged), unsupported cells
//! always fail closed.
//!
//! ```text
//! CLI lane → dep_gate::gate(core, op, tool, compat) → native engine
//!                                                      → compat toolchain (warned + logged)
//!                                                      → Unsupported error
//! ```
//! CẤM: CLI → cargo/uv/pip/flutter/gradle/pod/go ở dependency-op khi chưa
//! qua gate (mọi lane dependency PHẢI gọi gate trước khi spawn hoặc gọi
//! adapter).

use anyhow::Result;

use crate::commands::compat::{COMPAT_RUNTIMES, CompatMode};

/// Dependency-lifecycle operations covered by the firewall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepOp {
    Install,
    Add,
    Remove,
    Update,
    List,
}

impl DepOp {
    /// CLI-canonical operation name — matches user-facing verbs.
    /// Tên operation chuẩn CLI — khớp động từ user gọi.
    pub fn as_str(&self) -> &'static str {
        match self {
            DepOp::Install => "install",
            DepOp::Add => "add",
            DepOp::Remove => "remove",
            DepOp::Update => "update",
            DepOp::List => "list",
        }
    }
}

/// Ownership of one (core, op) cell — the ONLY source for gate decisions.
/// Capability matrices and docs must derive from this table, never
/// hardcode their own copy.
/// Quyền sở hữu của một ô (core, op) — nguồn DUY NHẤT cho quyết định gate.
/// Matrix capability và docs phải suy ra từ bảng này, không hardcode bản
/// sao riêng.
pub enum DepOwner {
    /// MGC owns the full lifecycle — proceeds in every mode.
    Native,
    /// An external toolchain owns it — compat opt-in only, never support.
    Delegated { tools: &'static [&'static str] },
    /// No lifecycle at all — always fails closed, even under compat.
    Unsupported,
}

/// Static ownership table (V1.2 §6.3 evidence baseline, HEAD 030ee69b).
/// Per (core, language, operation) — never per-core alone: a native
/// resolve engine does NOT imply a native install/add lane (the gate
/// measures the LANE the user invokes, not the engine's theoretical
/// capability).
/// Bảng sở hữu tĩnh (baseline §6.3). Theo (core, language, operation) —
/// không bao giờ theo core đơn độc: engine resolve native không suy ra
/// lane install/add native (gate đo LANE user gọi, không đo khả năng lý
/// thuyết của engine).
pub fn owner_for(core: &str, language: Option<&str>, op: DepOp) -> DepOwner {
    match (core, language, op) {
        // Web JS/TS: the native npm pipeline (resolve/lock/fetch/CAS/
        // materialize/lifecycle/audit) — the only fully native engine.
        ("web", _, _) => DepOwner::Native,
        // Lib TypeScript rides the embedded web engine end to end.
        ("lib", Some("ts"), _) => DepOwner::Native,
        // Lib list reads manifests/locks — no toolchain spawn.
        ("lib", _, DepOp::List) => DepOwner::Native,
        // Lib install is native for every resolved language: protocol
        // resolve (crates/PyPI/Go/Maven/NuGet) + verified fetch + CAS
        // materialize into toolchain-compatible layouts — zero toolchain
        // spawns in adapters/lib/src/install/ (verified by audit).
        ("lib", _, DepOp::Install) => DepOwner::Native,
        // Lib add/remove/update edit through the toolchain (cargo add /
        // pip install / go get with honest DELEGATED markers) — except
        // TypeScript, which rides the web delegate above.
        ("lib", _, _) => DepOwner::Delegated {
            tools: &["cargo", "uv", "pip", "pip3", "go"],
        },
        // AI Python/model: uv/pip own the lifecycle.
        ("ai", _, _) => DepOwner::Delegated {
            tools: &["uv", "pip", "pip3"],
        },
        // React Native has per-tier engines but NO install runner — the
        // invoked lane errors before any spawn, so no install lifecycle
        // exists to support (Phase C may add one).
        // (React Native có engine per-tier nhưng KHÔNG có runner install —
        // lane gọi lỗi trước mọi spawn, nên không có lifecycle install nào
        // để hỗ trợ.)
        ("app", Some("rn"), _) => DepOwner::Unsupported,
        // App Flutter/Swift/Kotlin/ObjC tiers: provider toolchains own
        // install/add/remove/update/list even where a native resolve
        // engine exists (the lane does not use it).
        ("app", _, _) => DepOwner::Delegated {
            tools: &["flutter", "gradle", "swift", "xcodebuild", "pod"],
        },
        // Game engines own their graphs (Bevy delegates to cargo).
        ("game", _, _) => DepOwner::Delegated { tools: &["cargo"] },
        // IoT frameworks own theirs (esp32-rust/cargo, pio, zephyr/west).
        ("iot", _, _) => DepOwner::Delegated {
            tools: &["cargo", "pio", "platformio", "west"],
        },
        // Cloud terraform: CDK/Pulumi ride the web engine (native —
        // those lanes branch BEFORE the gate and never call it).
        ("clo", _, _) => DepOwner::Delegated {
            tools: &["terraform"],
        },
        // CI/CD and hardware have no package lifecycle (scaffold/pipeline
        // and template/generator only) — compat cannot open what does not
        // exist.
        ("cicd", _, _) | ("hardware", _, _) => DepOwner::Unsupported,
        // Unknown core: fail closed, never default-open.
        _ => DepOwner::Unsupported,
    }
}

/// Full tool universe accepted by `--compat-runtime` on dependency
/// operations: PM/toolchain tools plus the rival JS runtimes (a wrong
/// runtime for the lane fails at the gate with a naming error, never
/// silently).
/// Toàn bộ tool chấp nhận ở `--compat-runtime` cho dependency-op.
pub const DEPENDENCY_COMPAT_TOOLS: &[&str] = &[
    "cargo",
    "uv",
    "pip",
    "pip3",
    "go",
    "flutter",
    "dart",
    "gradle",
    "mvn",
    "dotnet",
    "swift",
    "pod",
    "xcodebuild",
    "terraform",
    "pio",
    "platformio",
    "west",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "deno",
];

/// Languages with a potential per-language ownership split, enumerated by
/// `capabilities --json` so the matrix cross-check sees the same table
/// the firewall enforces (only differing languages are emitted).
/// (Ngôn ngữ có thể tách sở hữu riêng, để JSON capabilities thấy đúng
/// bảng tường lửa cưỡng chế — chỉ emit ngôn ngữ khác biệt.)
pub const SPLIT_LANGUAGES: &[&str] = &[
    "ts", "rust", "python", "go", "java", "dotnet", "flutter", "swift", "kotlin", "objc", "rn",
];

/// Parse `--compat-runtime` for a dependency operation: explicit flag wins,
/// otherwise the `MGC_COMPAT_RUNTIME` env (same explicit opt-in semantics
/// as the dev/test/run/build lanes), otherwise native.
///
/// Unlike `CompatMode::from_flag` (rival JS runtimes only), dependency
/// lanes accept the toolchain universe — the gate, not the parser, decides
/// which tool owns the lane.
/// Parse `--compat-runtime` cho dependency-op: cờ tường minh thắng, rồi
/// env, rồi native. Khác `from_flag` (chỉ runtime đối thủ), lane dependency
/// chấp nhận universe toolchain — gate (không phải parser) quyết định tool
/// nào sở hữu lane.
pub fn from_dep_flag(flag: Option<&str>) -> Result<CompatMode> {
    match flag {
        None => match std::env::var("MGC_COMPAT_RUNTIME").ok() {
            None => Ok(CompatMode::Native),
            Some(value) => parse_dep_value(&value),
        },
        Some(value) => parse_dep_value(value),
    }
}

/// Validate one compat value against the dependency tool universe.
/// (Kiểm tra một giá trị compat thuộc universe toolchain dependency.)
fn parse_dep_value(value: &str) -> Result<CompatMode> {
    let lowered = value.to_ascii_lowercase();
    if !DEPENDENCY_COMPAT_TOOLS.contains(&lowered.as_str())
        && !COMPAT_RUNTIMES.contains(&lowered.as_str())
    {
        return Err(crate::error::dep_gate_invalid_tool(
            value,
            DEPENDENCY_COMPAT_TOOLS,
        ));
    }
    Ok(CompatMode::Explicit(lowered))
}

/// The single control path: MUST be called by every dependency lane before
/// spawning a toolchain or routing to an adapter.
/// - `language`: detected project language for per-language splits
///   (`Some("ts")` for TypeScript lib projects), else `None`.
/// - `tool`: the lane's ACTUAL tool (`Some`), or `None` when the lane
///   routes to an adapter and only the owner table's tool set applies.
/// - `audit_log`: project `.magicore/exec.log` path for the compat-use
///   record (RULE §11: escape hatches announce themselves AND log).
///
/// Đường điều khiển duy nhất: mọi lane dependency PHẢI gọi trước khi
/// spawn toolchain hoặc gọi adapter.
pub fn gate_full(
    core: &str,
    language: Option<&str>,
    op: DepOp,
    tool: Option<&str>,
    compat: &CompatMode,
    audit_log: Option<&std::path::Path>,
) -> Result<()> {
    match owner_for(core, language, op) {
        DepOwner::Native => {
            if let CompatMode::Explicit(other) = compat {
                mgc_ui::info(&format!(
                    "`{core}` {} is MagiCore-native — ignoring `--compat-runtime {other}` (native engine always runs).",
                    op.as_str()
                ));
            }
            Ok(())
        }
        DepOwner::Unsupported => Err(crate::error::dep_gate_unsupported(core, op.as_str())),
        DepOwner::Delegated { tools } => match compat {
            CompatMode::Native => Err(crate::error::dep_gate_requires_compat(
                core,
                op.as_str(),
                tools,
            )),
            CompatMode::Explicit(wanted) => {
                let allowed = match tool {
                    Some(actual) => {
                        if !tools.contains(&actual) {
                            return Err(crate::error::dep_gate_wrong_tool(
                                core,
                                op.as_str(),
                                actual,
                                tools,
                            ));
                        }
                        wanted == actual
                    }
                    None => tools.contains(&wanted.as_str()),
                };
                if !allowed {
                    let got = tool.unwrap_or(wanted.as_str());
                    return Err(crate::error::dep_gate_wrong_tool(
                        core,
                        op.as_str(),
                        got,
                        tools,
                    ));
                }
                mgc_ui::warning(&format!(
                    "COMPATIBILITY MODE: `{core}` {} delegates to toolchain '{}' — this is NOT the native MagiCore engine path and is excluded from native-support claims.",
                    op.as_str(),
                    tool.unwrap_or(wanted.as_str())
                ));
                audit_compat_use(core, op, tool.unwrap_or(wanted.as_str()), audit_log);
                Ok(())
            }
        },
    }
}

/// Language-unaware shorthand: `gate_full` with no language split.
/// (Dạng gọn không-ngôn-ngữ.)
pub fn gate(
    core: &str,
    op: DepOp,
    tool: Option<&str>,
    compat: &CompatMode,
    audit_log: Option<&std::path::Path>,
) -> Result<()> {
    gate_full(core, None, op, tool, compat, audit_log)
}

/// Best-effort compat-use record: a failed audit write warns loudly but
/// never fails the operation itself (the gate decision already stands).
/// Ghi nhận compat best-effort: ghi audit lỗi thì cảnh báo lớn nhưng không
/// bao giờ làm fail operation (quyết định gate đã chốt).
fn audit_compat_use(core: &str, op: DepOp, tool: &str, audit_log: Option<&std::path::Path>) {
    let Some(path) = audit_log else {
        return;
    };
    let entry = mgc_exec::audit::AuditEntry {
        cmd: format!("mgc {} --compat-runtime {}", op.as_str(), tool),
        args: vec![format!("core={core}")],
        cwd: std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        exit_code: 0,
        duration_ms: 0,
        dry_run: false,
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    if let Err(err) = mgc_exec::audit::append(path, &entry) {
        mgc_ui::warning(&format!(
            "compat-use audit record could not be written to {}: {err:#} (the compat run itself is unaffected)",
            path.display()
        ));
    }
}

#[cfg(test)]
#[path = "test/dep_gate_test.rs"]
mod tests;
