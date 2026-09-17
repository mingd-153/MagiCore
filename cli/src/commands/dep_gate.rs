//! `dep_gate.rs` — C0 ownership firewall: the single control path for
//! dependency-lifecycle operations (T0.3, V1.2 Native Ownership Contract).
//!
//! Every dependency lane (install/add/remove/update/list) MUST pass through
//! `gate()` with its full `DepContext` before spawning a toolchain or
//! routing to an adapter:
//! native cells proceed, delegated cells require an explicit
//! `--compat-runtime` opt-in (warned + audit-logged), unsupported cells
//! always fail closed.
//!
//! ```text
//! CLI lane → dep_gate::gate(&DepContext{core, ecosystem, ..}, tool, compat) → native engine
//!                                                                              → compat toolchain (warned + logged)
//!                                                                              → Unsupported error
//! ```
//! CẤM: CLI → cargo/uv/pip/flutter/gradle/pod/go ở dependency-op khi chưa
//! qua gate (mọi lane dependency PHẢI gọi gate trước khi spawn hoặc gọi
//! adapter).

use anyhow::Result;

use crate::commands::compat::{COMPAT_RUNTIMES, CompatMode};

/// Dependency-lifecycle operations covered by the firewall: the five
/// user verbs plus the native pipeline stages a real package manager
/// owns (resolve → lock → fetch → verify → store → materialize, plus
/// frozen/offline reinstalls and store GC/recovery).
/// (Các operation lifecycle dependency tường lửa phủ: năm động từ user
/// cộng các giai đoạn pipeline PM native sở hữu.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepOp {
    Install,
    Add,
    Remove,
    Update,
    List,
    Resolve,
    Lock,
    Fetch,
    Verify,
    Store,
    Materialize,
    FrozenInstall,
    OfflineReinstall,
    Gc,
}

impl DepOp {
    /// CLI-canonical operation name — matches user-facing verbs; pipeline
    /// stages use their canonical stage names.
    /// Tên operation chuẩn CLI — khớp động từ user gọi.
    pub fn as_str(&self) -> &'static str {
        match self {
            DepOp::Install => "install",
            DepOp::Add => "add",
            DepOp::Remove => "remove",
            DepOp::Update => "update",
            DepOp::List => "list",
            DepOp::Resolve => "resolve",
            DepOp::Lock => "lock",
            DepOp::Fetch => "fetch",
            DepOp::Verify => "verify",
            DepOp::Store => "store",
            DepOp::Materialize => "materialize",
            DepOp::FrozenInstall => "frozen-install",
            DepOp::OfflineReinstall => "offline-reinstall",
            DepOp::Gc => "gc",
        }
    }
}

/// Full dependency-operation context — EVERY dependency lane MUST build
/// one before calling `gate()`. There is no language-unaware shorthand:
/// an undetermined ecosystem fails Unsupported, never falls back to a
/// generic delegated lane.
/// (Context đầy đủ cho operation dependency — MỌI lane dependency PHẢI
/// dựng một cái trước khi gọi `gate()`. Không có dạng gọn bỏ-ngôn-ngữ:
/// ecosystem không xác định được thì fail Unsupported, không fallback
/// sang lane delegated chung.)
#[derive(Debug, Clone, Copy)]
pub struct DepContext<'a> {
    /// CLI core: web | lib | ai | app | game | iot | clo | cicd | hardware.
    pub core: &'a str,
    /// Language/ecosystem id (canonical `eco::*` strings): the lane's
    /// DETECTED project ecosystem. `None`/unknown ⇒ Unsupported.
    /// (Ecosystem project lane DETECT được. Không có/không rõ ⇒ Unsupported.)
    pub ecosystem: Option<&'a str>,
    /// Framework id where the lane knows one (iot framework, else None).
    pub framework: Option<&'a str>,
    /// Deploy/materialize target where the lane knows one (iot board).
    pub target: Option<&'a str>,
    /// The invoked operation (verb or pipeline stage).
    pub op: DepOp,
}

impl<'a> DepContext<'a> {
    /// Build the mandatory gate context for one dependency operation.
    /// (Dựng context gate bắt buộc cho một operation dependency.)
    pub fn new(
        core: &'a str,
        ecosystem: Option<&'a str>,
        framework: Option<&'a str>,
        target: Option<&'a str>,
        op: DepOp,
    ) -> Self {
        Self {
            core,
            ecosystem,
            framework,
            target,
            op,
        }
    }

    /// Human/gate-log lane description: `core` plus the detected
    /// ecosystem, framework and target when known
    /// (`iot[esp32-rust|board:esp32]`-style precision for audit lines and
    /// user-facing messages).
    /// (Mô tả lane cho log: core kèm ecosystem/framework/target khi biết.)
    pub fn describe(&self) -> String {
        let mut out = self.core.to_string();
        if let Some(eco) = self.ecosystem {
            out.push_str(&format!("[{eco}"));
            if let Some(fw) = self.framework {
                out.push_str(&format!("|{fw}"));
            }
            if let Some(target) = self.target {
                out.push_str(&format!("|target:{target}"));
            }
            out.push(']');
        }
        out
    }
}

/// Canonical ecosystem ids — the ONLY strings the firewall matches on.
/// Lanes MUST map their detected language/framework to these (via the
/// adapter enums' `ecosystem()`), never pass display names through:
/// `AppLanguage::as_str()` yields "react-native" but the gate id is "rn".
/// (Id ecosystem chuẩn — chuỗi DUY NHẤT tường lửa khớp. Lane PHẢI map
/// ngôn ngữ detect được sang các id này, không bao giờ truyền tên hiển
/// thị: `as_str()` cho "react-native" nhưng id gate là "rn".)
pub mod eco {
    pub const JS: &str = "js";
    pub const TS: &str = "ts";
    pub const PYTHON: &str = "python";
    pub const RUST: &str = "rust";
    pub const GO: &str = "go";
    pub const JAVA: &str = "java";
    pub const DOTNET: &str = "dotnet";
    pub const FLUTTER: &str = "flutter";
    pub const KOTLIN: &str = "kotlin";
    pub const SWIFT: &str = "swift";
    pub const OBJC: &str = "objc";
    /// React Native gate id (NOT "react-native").
    pub const RN: &str = "rn";
    // NOTE: no MULTI const — a multi-platform project never gates as a
    // unit (install_multi gates per platform with android/ios/flutter/
    // react-native ids); passing "multi" falls to Unsupported by design.
    pub const BEVY: &str = "bevy";
    pub const TERRAFORM: &str = "terraform";
}

/// Ownership of one (core, ecosystem, operation) cell — the ONLY source
/// for gate decisions.
/// Capability matrices and docs must derive from this table, never
/// hardcode their own copy.
/// Quyền sở hữu của một ô (core, ecosystem, operation) — nguồn DUY NHẤT
/// cho quyết định gate. Matrix capability và docs phải suy ra từ bảng
/// này, không hardcode bản sao riêng.
pub enum DepOwner {
    /// MGC owns the full lifecycle — proceeds in every mode.
    Native,
    /// An external toolchain owns it — compat opt-in only, never support.
    Delegated { tools: &'static [&'static str] },
    /// No package lifecycle, but a REAL read-only inventory command
    /// exists (hardware list) — passes the gate like Native, reported as
    /// "scaffold-only" (never "mgc-native") in capabilities. A compat
    /// opt-in on such a cell is accepted for CLI uniformity and ignored
    /// with a notice.
    ScaffoldOnly,
    /// No lifecycle at all — always fails closed, even under compat.
    Unsupported,
}

/// Static ownership table (V1.2 §6.3 evidence baseline, HEAD 030ee69b).
/// Matched on (core, ecosystem, operation) — never on core alone, and
/// never one label for a whole ecosystem: an operation WITHOUT a real
/// runner is Unsupported, not Delegated (a "delegated" cell for a command
/// that does not exist is a false capability, not a cautious one).
/// Every arm names an explicit ecosystem; the catch-all is Unsupported —
/// an undetermined ecosystem fails closed, never falls back to a generic
/// delegated lane.
///
/// Pipeline stages (resolve/lock/fetch/verify/store/materialize/frozen/
/// offline/gc) share their (core, ecosystem)'s INSTALL ownership: the
/// engine/toolchain that owns install owns the pipeline. Only the five
/// user verbs plus List keep distinct cells.
/// Bảng sở hữu tĩnh (baseline §6.3). Khớp theo (core, ecosystem,
/// operation) — operation không có runner thật là Unsupported, không
/// phải Delegated.
pub fn owner_for(ctx: &DepContext) -> DepOwner {
    // Pipeline stages ride Install ownership; the verbs + List stay exact.
    // (Stage pipeline đi theo ownership của Install.)
    let op = match ctx.op {
        DepOp::Resolve
        | DepOp::Lock
        | DepOp::Fetch
        | DepOp::Verify
        | DepOp::Store
        | DepOp::Materialize
        | DepOp::FrozenInstall
        | DepOp::OfflineReinstall
        | DepOp::Gc => DepOp::Install,
        verb => verb,
    };
    match (ctx.core, ctx.ecosystem, op) {
        // Web JS/TS: the native npm pipeline (resolve/lock/fetch/CAS/
        // materialize/lifecycle/audit) — the only fully native engine.
        // "javascript"/"typescript" are accepted as the same engine's
        // common names (the lifecycle matrix taxonomy uses them).
        ("web", Some(eco::JS | eco::TS | "javascript" | "typescript"), _) => DepOwner::Native,
        // Lib TypeScript rides the embedded web engine end to end.
        ("lib", Some(eco::TS), _) => DepOwner::Native,
        // Lib protocol languages: the native pipeline owns install and
        // every stage EXCEPT the mutating verbs below, which spawn the
        // provider toolchain for real (verified per arm against
        // adapters/lib/src/adapter.rs).
        ("lib", Some(eco::RUST), DepOp::Add | DepOp::Remove | DepOp::Update) => {
            DepOwner::Delegated { tools: &["cargo"] }
        }
        // Python edits spawn PIP for real (adapter hardcodes pip) — `uv`
        // is NOT in the set: a uv opt-in running pip would break the
        // flag==process contract.
        ("lib", Some(eco::PYTHON), DepOp::Add | DepOp::Remove | DepOp::Update) => {
            DepOwner::Delegated {
                tools: &["pip", "pip3"],
            }
        }
        // Go: `go get` / `go get -u` spawn for real; removal has NO
        // runner (honest manual `go mod tidy` step) — Unsupported.
        ("lib", Some(eco::GO), DepOp::Add | DepOp::Update) => {
            DepOwner::Delegated { tools: &["go"] }
        }
        ("lib", Some(eco::GO), DepOp::Remove) => DepOwner::Unsupported,
        // Java/.NET: add/remove/update have NO runner (honest manual
        // gradle/dotnet steps) — Unsupported until a real runner exists.
        ("lib", Some(eco::JAVA | eco::DOTNET), DepOp::Add | DepOp::Remove | DepOp::Update) => {
            DepOwner::Unsupported
        }
        (
            "lib",
            Some(eco::RUST | eco::PYTHON | eco::GO | eco::JAVA | eco::DOTNET),
            _,
        ) => DepOwner::Native,
        // Lib list reads manifests/locks through the adapter — spawn-free
        // whatever the language, so it stays native even undeclared.
        ("lib", _, DepOp::List) => DepOwner::Native,
        // game/iot/clo list: adapter manifest reads, spawn-free like lib
        // list — native whatever the ecosystem. (The MUTATING lanes above
        // still require declared ecosystems; list cannot spawn by
        // construction, verified by the delegation audit.)
        ("game" | "iot" | "clo", _, DepOp::List) => DepOwner::Native,
        // AI Python: uv/pip own the whole lifecycle INCLUDING list
        // (`uv pip list` / `pip list` spawn the provider tool).
        ("ai", Some(eco::PYTHON), _) => DepOwner::Delegated {
            tools: &["uv", "pip", "pip3"],
        },
        // React Native has per-tier engines but NO install runner — the
        // invoked lane errors before any spawn, so no install lifecycle
        // exists to support (Phase C may add one).
        // (React Native có engine per-tier nhưng KHÔNG có runner install —
        // lane gọi lỗi trước mọi spawn, nên không có lifecycle install nào
        // để hỗ trợ.)
        ("app", Some(eco::RN), _) => DepOwner::Unsupported,
        // App Flutter: provider toolchain owns every verb (tool_command
        // implements add/remove/list/update; install runs flutter pub).
        ("app", Some(eco::FLUTTER), _) => DepOwner::Delegated {
            tools: &["flutter", "gradle", "swift", "xcodebuild", "pod"],
        },
        // App Swift: install + list runners exist; add/remove/update have
        // NO command — Unsupported (never a delegated promise).
        ("app", Some(eco::SWIFT), DepOp::Add | DepOp::Remove | DepOp::Update) => {
            DepOwner::Unsupported
        }
        ("app", Some(eco::SWIFT), _) => DepOwner::Delegated {
            tools: &["flutter", "gradle", "swift", "xcodebuild", "pod"],
        },
        // App Kotlin: install + list runners exist; add/remove/update have
        // NO command — Unsupported.
        ("app", Some(eco::KOTLIN), DepOp::Add | DepOp::Remove | DepOp::Update) => {
            DepOwner::Unsupported
        }
        ("app", Some(eco::KOTLIN), _) => DepOwner::Delegated {
            tools: &["flutter", "gradle", "swift", "xcodebuild", "pod"],
        },
        // App ObjC: ONLY the xcodebuild install runner exists;
        // add/remove/update/list have NO command — Unsupported.
        ("app", Some(eco::OBJC), DepOp::Install) => DepOwner::Delegated {
            tools: &["flutter", "gradle", "swift", "xcodebuild", "pod"],
        },
        ("app", Some(eco::OBJC), _) => DepOwner::Unsupported,
        // Game bevy lane: cargo owns the graph.
        ("game", Some(eco::BEVY), _) => DepOwner::Delegated { tools: &["cargo"] },
        // IoT frameworks own theirs (esp32-rust/cargo, pio, zephyr/west).
        // The ecosystem slot carries the detected framework id — the iot
        // lane has no separate language layer.
        ("iot", Some("esp32-rust" | "platformio" | "zephyr"), _) => DepOwner::Delegated {
            tools: &["cargo", "pio", "platformio", "west"],
        },
        // Cloud terraform: CDK/Pulumi ride the web engine (native —
        // those lanes branch BEFORE the gate and never call it).
        ("clo", Some(eco::TERRAFORM), _) => DepOwner::Delegated {
            tools: &["terraform"],
        },
        // Hardware list is a REAL read-only inventory command over
        // optimizer/bench templates — no package lifecycle, so it is
        // ScaffoldOnly (passes the gate, reported as such, never
        // "mgc-native").
        ("hardware", _, DepOp::List) => DepOwner::ScaffoldOnly,
        // CI/CD and hardware have no package lifecycle (scaffold/pipeline
        // and template/generator only) — compat cannot open what does not
        // exist.
        ("cicd", _, _) | ("hardware", _, _) => DepOwner::Unsupported,
        // Unknown core / ecosystem / combination: fail closed, never
        // default-open.
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
/// "javascript" is included because the lifecycle matrix taxonomy names
/// the web lane's language that way (gate id "js" is the lane-passed id);
/// "bevy" / the iot framework ids / "terraform" are the delegated lanes'
/// gate ecosystems (the matrix cross-check resolves (game,rust) → bevy
/// and (iot,rust) → esp32-rust through them).
/// (Ngôn ngữ có thể tách sở hữu riêng, để JSON capabilities thấy đúng
/// bảng tường lửa cưỡng chế — chỉ emit ngôn ngữ khác biệt.)
pub const SPLIT_LANGUAGES: &[&str] = &[
    "ts", "js", "rust", "python", "go", "java", "dotnet", "flutter", "swift", "kotlin", "objc",
    "rn", "javascript", "bevy", "esp32-rust", "platformio", "zephyr", "terraform",
];

/// Same-tool alias equivalence for the gate's exact-match check: `pip`
/// and `pip3` are the same lifecycle owner (likewise `pio` /
/// `platformio`). A `--compat-runtime pip3` opt-in therefore opens a
/// `pip`-resolved lane and vice versa — but NEVER a different owner
/// (`uv` vs `pip` still mismatches). Without this, pip3-only hosts
/// could not opt in honestly.
/// (Alias cùng-tool cho kiểm tra khớp của gate: pip/pip3 là cùng owner.)
fn same_tool(wanted: &str, actual: &str) -> bool {
    wanted == actual
        || matches!(
            (wanted, actual),
            ("pip", "pip3")
                | ("pip3", "pip")
                | ("pio", "platformio")
                | ("platformio", "pio")
        )
}
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

/// The single control path: MUST be called by every dependency lane
/// before spawning a toolchain or routing to an adapter, with the lane's
/// full `DepContext` (core + detected ecosystem + framework + target +
/// operation). There is deliberately NO language-unaware shorthand —
/// callers that cannot determine the ecosystem fail Unsupported.
/// - `tool`: the lane's ACTUAL tool (`Some`), or `None` when the lane
///   routes to an adapter and only the owner table's tool set applies.
/// - `audit_log`: project `.magicore/exec.log` path for the compat-use
///   record (RULE §11: escape hatches announce themselves AND log).
///
/// Đường điều khiển duy nhất: mọi lane dependency PHẢI gọi trước khi
/// spawn toolchain hoặc gọi adapter, kèm `DepContext` đầy đủ. Cố ý KHÔNG
/// có dạng gọn bỏ-ngôn-ngữ — caller không xác định được ecosystem thì
/// fail Unsupported.
pub fn gate(
    ctx: &DepContext,
    tool: Option<&str>,
    compat: &CompatMode,
    audit_log: Option<&std::path::Path>,
) -> Result<()> {
    match owner_for(ctx) {
        DepOwner::Native => {
            if let CompatMode::Explicit(other) = compat {
                mgc_ui::info(&format!(
                    "`{}` {} is MagiCore-native — ignoring `--compat-runtime {other}` (native engine always runs).",
                    ctx.describe(),
                    ctx.op.as_str()
                ));
            }
            Ok(())
        }
        DepOwner::ScaffoldOnly => {
            if let CompatMode::Explicit(other) = compat {
                mgc_ui::info(&format!(
                    "`{}` {} is a scaffold-only lane (inventory/templates, no package lifecycle) — ignoring `--compat-runtime {other}`.",
                    ctx.describe(),
                    ctx.op.as_str()
                ));
            }
            Ok(())
        }
        DepOwner::Unsupported => Err(crate::error::dep_gate_unsupported(
            ctx.core,
            ctx.op.as_str(),
            ctx.ecosystem,
        )),
        DepOwner::Delegated { tools } => match compat {
            CompatMode::Native => Err(crate::error::dep_gate_requires_compat(
                ctx.core,
                ctx.op.as_str(),
                ctx.ecosystem,
                tools,
            )),
            CompatMode::Explicit(wanted) => {
                let allowed = match tool {
                    Some(actual) => {
                        if !tools.contains(&actual) {
                            return Err(crate::error::dep_gate_lane_tool_not_owned(
                                ctx.core,
                                ctx.op.as_str(),
                                ctx.ecosystem,
                                actual,
                                tools,
                            ));
                        }
                        same_tool(wanted, actual)
                    }
                    None => tools.contains(&wanted.as_str()),
                };
                if !allowed {
                    // Exact-tool lane, wrong family member: name BOTH so
                    // the flag==process mismatch is visible (never report
                    // the lane's tool as if the user had passed it).
                    if let Some(actual) = tool {
                        return Err(crate::error::dep_gate_tool_mismatch(
                            ctx.core,
                            ctx.op.as_str(),
                            ctx.ecosystem,
                            wanted,
                            actual,
                            tools,
                        ));
                    }
                    return Err(crate::error::dep_gate_wrong_tool(
                        ctx.core,
                        ctx.op.as_str(),
                        ctx.ecosystem,
                        wanted,
                        tools,
                    ));
                }
                mgc_ui::warning(&format!(
                    "COMPATIBILITY MODE: `{}` {} delegates to toolchain '{}' — this is NOT the native MagiCore engine path and is excluded from native-support claims.",
                    ctx.describe(),
                    ctx.op.as_str(),
                    tool.unwrap_or(wanted.as_str())
                ));
                audit_compat_use(ctx.core, ctx.op, tool.unwrap_or(wanted.as_str()), audit_log);
                Ok(())
            }
        },
    }
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
