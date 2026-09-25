//! `dep_gate.rs` — C0 ownership firewall: the single control path for
//! dependency-lifecycle operations (T0.3, V1.2 Native Ownership Contract).
//!
//! Every dependency lane (install/add/remove/update/list) MUST pass through
//! `gate()` with its full `DepContext` before spawning a toolchain or
//! routing to an adapter:
//! native cells proceed; delegated and unsupported cells fail closed.
//!
//! ```text
//! CLI lane → dep_gate::gate(&DepContext{core, ecosystem, ..}, tool, compat) → native engine or Unsupported error
//! ```
//! CẤM: CLI → cargo/uv/pip/flutter/gradle/pod/go ở dependency-op khi chưa
//! qua gate (mọi lane dependency PHẢI gọi gate trước khi spawn hoặc gọi
//! adapter).

use anyhow::Result;

use crate::commands::compat::CompatMode;

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
    /// Canonical operation inventory shared by every capability surface.
    /// Danh sách operation chuẩn dùng chung cho mọi bề mặt capability.
    pub const ALL: &'static [DepOp] = &[
        DepOp::Install,
        DepOp::Add,
        DepOp::Remove,
        DepOp::Update,
        DepOp::List,
        DepOp::Resolve,
        DepOp::Lock,
        DepOp::Fetch,
        DepOp::Verify,
        DepOp::Store,
        DepOp::Materialize,
        DepOp::FrozenInstall,
        DepOp::OfflineReinstall,
        DepOp::Gc,
    ];

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
        // Lib Remove AND Update run natively on mgc-written manifests
        // (remove: in-memory edit + writer; update: resolve-latest +
        // rewrite + native install tail; zero spawn — gradle projects
        // fail closed in write_manifest/prepare_add).
        // (Remove/Update native.)
        (
            "lib",
            Some(eco::RUST | eco::PYTHON | eco::GO | eco::DOTNET | eco::JAVA),
            DepOp::Remove | DepOp::Update,
        ) => DepOwner::Native,
        // .NET Add/Remove run natively (NuGet resolve-first + mgc-side
        // csproj edit, zero `dotnet` spawn); Update has NO runner —
        // Unsupported. Java Add/Remove run natively for pom.xml projects
        // (Maven resolve-first + mgc-side pom edit); gradle projects fail
        // closed inside prepare_add/write_manifest (scripts are programs).
        // Java Update stays Unsupported.
        // (Add/Remove .NET/Java-pom native.)
        ("lib", Some(eco::DOTNET), DepOp::Add) => DepOwner::Native,
        ("lib", Some(eco::JAVA), DepOp::Add) => DepOwner::Native,
        // A native list needs verified installed-state evidence, not just
        // a manifest/lock pin. Python has an MGC environment inventory;
        // TypeScript uses the Web adapter's node_modules verifier. The
        // other lib lanes remain unsupported until they publish one.
        ("lib", Some(eco::PYTHON), DepOp::List) => DepOwner::Native,
        ("lib", Some(eco::RUST | eco::GO | eco::JAVA | eco::DOTNET), DepOp::List) => {
            DepOwner::Unsupported
        }
        ("lib", Some(eco::RUST | eco::PYTHON | eco::GO | eco::JAVA | eco::DOTNET), _) => {
            DepOwner::Native
        }
        // Game/IoT currently parse dependency declarations but cannot
        // verify installed state. Do not report requested ranges as
        // installed versions; no list lane is advertised yet.
        ("game" | "iot", _, DepOp::List) => DepOwner::Unsupported,
        // CDK/Pulumi package.json lanes use the embedded MGC Web engine.
        // A framework name alone is insufficient: callers must detect a JS
        // manifest and pass the matching ecosystem before entering this arm.
        ("clo", Some(eco::JS | eco::TS | "javascript" | "typescript"), _)
            if matches!(ctx.framework, Some("cdk" | "pulumi")) =>
        {
            DepOwner::Native
        }
        ("clo", Some(eco::TERRAFORM | "cloudflare"), DepOp::List) => DepOwner::Unsupported,
        // AI's typed PEP 621 lane uses the native resolver; CLI performs
        // project-specific manifest validation before entering this table.
        // Lane PEP 621 của AI dùng resolver native; CLI kiểm tra manifest trước.
        ("ai", Some(eco::PYTHON), DepOp::Install | DepOp::Add | DepOp::Update) => DepOwner::Native,
        // Project-specific native AI lanes are admitted by
        // `gate_native_adapter`; this static fallback never opens a PM.
        // Lane AI native được xét qua adapter; fallback tĩnh không mở PM.
        ("ai", Some(eco::PYTHON), _) => DepOwner::Unsupported,
        // React Native has per-tier engines but NO install runner — the
        // invoked lane errors before any spawn, so no install lifecycle
        // exists to support (Phase C may add one).
        // (React Native có engine per-tier nhưng KHÔNG có runner install —
        // lane gọi lỗi trước mọi spawn, nên không có lifecycle install nào
        // để hỗ trợ.)
        ("app", Some(eco::RN), _) => DepOwner::Unsupported,
        // MagiCore owns Flutter install/add/update; remove is native in the
        // rule just below. The remaining Flutter verbs are separate owner
        // cells and must not inherit dependency support.
        // (MagiCore sở hữu install/add/update Flutter; remove nằm ở rule
        // ngay dưới, verb còn lại không được kế thừa capability này.)
        ("app", Some(eco::FLUTTER), DepOp::Install | DepOp::Add | DepOp::Update) => {
            DepOwner::Native
        }
        // Flutter remove uses the journaled shared manifest path. Swift
        // source and Kotlin catalog edits are blocked until their writers
        // participate in the same crash-recovery transaction.
        // (Flutter remove có journal; Swift/Kotlin bị chặn tới khi có transaction.)
        ("app", Some(eco::FLUTTER), DepOp::Remove) => DepOwner::Native,
        ("app", Some(eco::FLUTTER), _) => DepOwner::Unsupported,
        // Swift install is native. Source mutations remain blocked until
        // lock/journal transaction support covers Package.swift.
        // (Swift install native; mutation source chưa transactional.)
        ("app", Some(eco::SWIFT), DepOp::Add | DepOp::Remove | DepOp::Update) => {
            DepOwner::Unsupported
        }
        ("app", Some(eco::SWIFT), DepOp::Install) => DepOwner::Native,
        ("app", Some(eco::SWIFT), _) => DepOwner::Unsupported,
        // Kotlin catalog edits are not journaled yet; no mutation is
        // advertised until crash-safe transaction support lands.
        // (Sửa catalog Kotlin chưa có journal; chưa quảng bá mutation.)
        ("app", Some(eco::KOTLIN), _) => DepOwner::Unsupported,
        // Objective-C dependency operations remain unsupported.
        ("app", Some(eco::OBJC), _) => DepOwner::Unsupported,
        // Only the CLI's Bevy + Cargo.toml branch is native. Other game
        // engines have no package operation implementation; never turn a
        // compatibility flag into a false adapter capability.
        // (Chỉ Bevy + Cargo.toml qua CLI là native; engine khác chưa có.)
        ("game", Some(eco::BEVY), DepOp::Install | DepOp::Add | DepOp::Update | DepOp::Remove) => {
            DepOwner::Native
        }
        ("game", Some(eco::BEVY), _) => DepOwner::Unsupported,
        ("game", _, _) => DepOwner::Unsupported,
        // IoT esp32-rust with a Cargo.toml installs/adds/updates/removes
        // natively (shared crates engine → mgc.lock, zero `cargo`
        // spawn). Other frameworks stay as below.
        // (esp32-rust có Cargo.toml thì native.)
        (
            "iot",
            Some("esp32-rust"),
            DepOp::Install | DepOp::Add | DepOp::Update | DepOp::Remove,
        ) => DepOwner::Native,
        // The supported ESP32-Rust CLI branch routes to the native Lib/Rust
        // engine. PlatformIO/Zephyr package operations remain unsupported
        // until MGC owns their resolver/store/materializer; installing a
        // toolchain command is not equivalent to adapter capability.
        // (ESP32-Rust dùng engine native Lib/Rust; PlatformIO/Zephyr chưa.)
        ("iot", Some("esp32-rust" | "platformio" | "zephyr"), _) => DepOwner::Unsupported,
        // Cloud Terraform has no native dependency lifecycle. CDK/Pulumi ride the web
        // engine (native — those lanes branch BEFORE the gate and never
        // call it).
        ("clo", Some(eco::TERRAFORM), _) => DepOwner::Unsupported,
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

/// No external runtime is accepted for dependency operations.
/// Không nhận runtime ngoài cho các thao tác dependency.
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
    "ts",
    "js",
    "rust",
    "python",
    "go",
    "java",
    "dotnet",
    "flutter",
    "swift",
    "kotlin",
    "objc",
    "rn",
    "javascript",
    "bevy",
    "esp32-rust",
    "platformio",
    "zephyr",
    "terraform",
];

/// Dependency operations are native-only: any explicit compat value is
/// rejected before a command can select a provider package manager.
/// Dependency chỉ chạy native; mọi compat value bị chặn trước khi chọn PM.
pub fn from_dep_flag(flag: Option<&str>) -> Result<CompatMode> {
    let explicit = flag
        .map(str::to_owned)
        .or_else(|| std::env::var("MGC_COMPAT_RUNTIME").ok());
    if let Some(value) = explicit {
        return Err(crate::error::dep_gate_compat_disabled(&value));
    }
    Ok(CompatMode::Native)
}

/// Gate cloud dependency operations only when the detected CDK/Pulumi
/// project actually has a JavaScript package manifest handled by the
/// embedded MGC Web engine. Terraform/other cloud types and manifest-less
/// projects remain Unsupported; this never opts into a provider PM.
/// (Chỉ mở lane CDK/Pulumi có package.json do engine Web nội bộ xử lý.)
pub fn gate_cloud_project(
    root: &std::path::Path,
    framework: &str,
    op: DepOp,
    compat_flag: Option<&str>,
) -> Result<()> {
    let ecosystem = (matches!(framework, "cdk" | "pulumi") && root.join("package.json").is_file())
        .then_some(eco::JS);
    let compat = from_dep_flag(compat_flag)?;
    gate(
        &DepContext::new("clo", ecosystem, Some(framework), None, op),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )
}

/// The single control path: MUST be called by every dependency lane
/// before spawning a toolchain or routing to an adapter, with the lane's
/// full `DepContext` (core + detected ecosystem + framework + target +
/// operation). There is deliberately NO language-unaware shorthand —
/// callers that cannot determine the ecosystem fail Unsupported.
/// - `tool` and `audit_log` remain in the signature for source compatibility;
///   neither can authorize an external package manager.
///
/// `tool` và `audit_log` giữ chữ ký tương thích; không tham số nào mở PM ngoài.
///
/// Đường điều khiển duy nhất: mọi lane dependency PHẢI gọi trước khi
/// spawn toolchain hoặc gọi adapter, kèm `DepContext` đầy đủ. Cố ý KHÔNG
/// có dạng gọn bỏ-ngôn-ngữ — caller không xác định được ecosystem thì
/// fail Unsupported.
pub fn gate(
    ctx: &DepContext,
    _tool: Option<&str>,
    compat: &CompatMode,
    _audit_log: Option<&std::path::Path>,
) -> Result<()> {
    if let CompatMode::Explicit(value) = compat {
        return Err(crate::error::dep_gate_compat_disabled(value));
    }
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
    }
}

/// Gate a dependency operation routed through an already-constructed
/// adapter whose identity and declared operation capability are checked
/// here. This supports project-specific native lanes (AI/Python with a
/// PEP 621 manifest) without changing the static owner of foreign-lock
/// compatibility lanes.
/// (Cổng native theo adapter đã dựng: xác minh identity + capability,
/// không mở nhầm nhánh foreign-lock delegated.)
pub fn gate_native_adapter(
    ctx: &DepContext<'_>,
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    compat: &CompatMode,
) -> Result<()> {
    if let CompatMode::Explicit(value) = compat {
        return Err(crate::error::dep_gate_compat_disabled(value));
    }
    let identity = adapter.manifest_identity().ok_or_else(|| {
        crate::error::dep_gate_unsupported(ctx.core, ctx.op.as_str(), ctx.ecosystem)
    })?;
    let expected_language = ctx.ecosystem.unwrap_or_default();
    if identity.core != ctx.core || identity.language != expected_language {
        return Err(anyhow::anyhow!(
            "native adapter identity mismatch for {} {}: expected {}/{}, got {}/{}",
            ctx.describe(),
            ctx.op.as_str(),
            ctx.core,
            expected_language,
            identity.core,
            identity.language
        ));
    }

    // Static native lanes remain governed by owner_for. The only dynamic
    // exception is ai/python remove+list: these are native only when this
    // verified adapter exposes the Python dependency/lock capabilities;
    // foreign-lock CLI paths continue through normal `gate()` as delegated.
    let dynamically_native = matches!(
        (ctx.core, ctx.ecosystem, ctx.op),
        ("ai", Some(eco::PYTHON), DepOp::Remove | DepOp::List)
    );
    if !matches!(owner_for(ctx), DepOwner::Native) && !dynamically_native {
        return Err(crate::error::dep_gate_unsupported(
            ctx.core,
            ctx.op.as_str(),
            ctx.ecosystem,
        ));
    }
    let required = match ctx.op {
        DepOp::Install => mgc_types::capabilities::Capability::ContentStoreProvider,
        DepOp::List => mgc_types::capabilities::Capability::LockfileProvider,
        DepOp::Add | DepOp::Remove | DepOp::Update => {
            mgc_types::capabilities::Capability::DependencyResolver
        }
        _ => {
            return Err(crate::error::dep_gate_unsupported(
                ctx.core,
                ctx.op.as_str(),
                ctx.ecosystem,
            ));
        }
    };
    if !adapter.capabilities().contains(&required)
        || (ctx.op == DepOp::Update && !adapter.supports_native_update())
    {
        return Err(crate::error::dep_gate_unsupported(
            ctx.core,
            ctx.op.as_str(),
            ctx.ecosystem,
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/dep_gate_test.rs"]
mod tests;
