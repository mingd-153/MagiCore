//! Framework qualification records — every wizard-selectable framework
//! id gets exactly one record stating what MagiCore may honestly claim
//! about it. Framework route and dependency-operation ownership are
//! separate: the latter is derived from `dep_gate::owner_for` for every
//! operation, never summarized as one framework-wide label.
//! A coverage test below parses `cli/src/wizard/*.rs` and fails when any
//! `Answer::new(.., "<id>")` lacks a record — new wizard entries cannot
//! silently arrive unqualified.
//!
//! (Hồ sơ qualification framework — mỗi id wizard chọn được có đúng một
//! record nêu điều MagiCore được phép claim trung thực. Test coverage
//! parse source wizard và fail khi id nào thiếu record.)

/// Scaffold/engine route only; dependency ownership is reported per operation.
/// Trạng thái scaffold/engine route; quyền dependency được báo theo operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameworkStatus {
    /// Offered by the wizard; lifecycle unqualified beyond scaffolding —
    /// no native/delegated/E2E claim.
    ScaffoldOnly,
    /// The framework branch routes through an MGC-owned engine path; this
    /// alone is not a claim that every dependency operation is supported.
    NativeEngine,
}

/// One qualification record: (core, wizard framework id) → claim.
/// (Một record qualification.)
pub struct FrameworkRecord {
    pub core: &'static str,
    pub framework: &'static str,
    pub status: FrameworkStatus,
    pub evidence: &'static str,
}

/// Shared evidence pointers (precise test/lane paths, no hand-waving).
/// (Căn cứ chung.)
pub const EV_WIZARD: &str = "wizard answer id only (lifecycle unqualified beyond scaffold)";
pub const EV_SCAFFOLD_WEB: &str =
    "cli/tests/web_fw_* + templates/backend.rs (scaffold proven, lifecycle unqualified)";
pub const EV_DEPGATE: &str =
    "dep_gate owner table + cli/tests/dep_gate_canary (zero-spawn proof per lane)";
pub const EV_LIB_PIPELINE: &str =
    "adapters/lib/src/install (spawn-free scan) + mgc-resolver protocols_* tests + dep_gate table";
pub const EV_WEB_ENGINE: &str =
    "npm registry pipeline (resolve/lock/fetch/CAS/materialize) + dep_gate web Native cell";
pub const EV_RN_GATE: &str =
    "dep_gate app/rn Unsupported cell + dep_gate_canary rn_* (no runner exists)";

/// Evidence from prior E2E runs. Historical spawn evidence does not prove
/// current dependency ownership; operation cells below come from dep_gate.
/// (E2E lịch sử không chứng minh ownership hiện tại; operation lấy từ dep_gate.)
pub const EV_E2E_APP_FLUTTER: &str = "Current native dependency E2E: `add-app fixture_pkg` resolves, verifies, writes pubspec+mgc.lock and installs the graph with zero Flutter/Dart spawn; Flutter list/dev and dependency-health outdated scan remain toolchain-owned. Historical delegated `flutter pub add` evidence is not current ownership proof.";
pub const EV_E2E_APP_SWIFT: &str = "Historical SwiftPM spawn is not current ownership evidence; see per-operation owner cells derived from dep_gate";
pub const EV_E2E_CLO_TERRAFORM: &str = "Terraform dependency operations are currently unsupported; deploy/tool execution is a separate capability and does not imply native package ownership";
pub const EV_E2E_HARDWARE: &str = "E2E template lane: create/add/install/list all exit 0 (optimizer.json/bench.json materialized); `mgc optimizer` applied profile.json + runtime envs on web+ai projects; `mgc bench` timed install; no package lifecycle by design";
pub const EV_E2E_IOT_PIO: &str = "Historical PlatformIO spawn is not current ownership evidence; current dependency operations are unsupported";
pub const EV_E2E_IOT_WEST: &str = "Historical West spawn is not current ownership evidence; current dependency operations are unsupported";
pub const EV_E2E_WEB_JS: &str = "scaffold + native install E2E in sandbox sweep (next build OK); status stays ScaffoldOnly (per-framework lifecycle unqualified)";
pub const EV_E2E_WEB_BACKEND: &str = "scaffold + install E2E: actix 82 pkgs, gin 58, echo 24, fiber 24, django 5, flask 10, axum 30; fastapi scaffold + resolve only (sandbox CDN blocked install — transient)";

/// Every wizard-selectable framework id, exactly once per core.
/// (Mọi id framework wizard chọn được, đúng một lần mỗi core.)
pub const RECORDS: &[FrameworkRecord] = &[
    // ── ai ──
    FrameworkRecord {
        core: "ai",
        framework: "python-agent",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_DEPGATE,
    },
    FrameworkRecord {
        core: "ai",
        framework: "mcp-server",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    // ── app ──
    FrameworkRecord {
        core: "app",
        framework: "flutter",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_APP_FLUTTER,
    },
    FrameworkRecord {
        core: "app",
        framework: "kotlin",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "dep_gate app/kotlin install+list cells + canary (add/remove/update: no runner)",
    },
    FrameworkRecord {
        core: "app",
        framework: "swift",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_APP_SWIFT,
    },
    FrameworkRecord {
        core: "app",
        framework: "react-native",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_RN_GATE,
    },
    FrameworkRecord {
        core: "app",
        framework: "maui",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "app",
        framework: "tauri",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "app",
        framework: "multi",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard branch node (per-platform lanes gate individually)",
    },
    // ── cicd: scaffold/pipeline only by design ──
    FrameworkRecord {
        core: "cicd",
        framework: "argocd",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "cicd",
        framework: "aws",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "cicd",
        framework: "cloudflare",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "cicd",
        framework: "gcp",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "cicd",
        framework: "github-actions",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    // ── cloud ──
    FrameworkRecord {
        core: "clo",
        framework: "terraform",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_CLO_TERRAFORM,
    },
    FrameworkRecord {
        core: "clo",
        framework: "cdk",
        status: FrameworkStatus::NativeEngine,
        evidence: "clo lane branches pre-gate into the embedded web engine",
    },
    FrameworkRecord {
        core: "clo",
        framework: "pulumi",
        status: FrameworkStatus::NativeEngine,
        evidence: "clo lane branches pre-gate into the embedded web engine",
    },
    FrameworkRecord {
        core: "clo",
        framework: "cloudflare",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    // ── game: only bevy has a runner (cargo); the rest fail closed ──
    FrameworkRecord {
        core: "game",
        framework: "bevy",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_DEPGATE,
    },
    FrameworkRecord {
        core: "game",
        framework: "godot",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "no package manager (lane fail-closed); canary godot_*",
    },
    FrameworkRecord {
        core: "game",
        framework: "unity",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "UPM CLI is P2 (lane fail-closed)",
    },
    FrameworkRecord {
        core: "game",
        framework: "unreal",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "no package manager (lane fail-closed)",
    },
    // ── hardware: inventory/templates only ──
    FrameworkRecord {
        core: "hardware",
        framework: "bench",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_HARDWARE,
    },
    FrameworkRecord {
        core: "hardware",
        framework: "optimizer",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_HARDWARE,
    },
    // ── iot ──
    FrameworkRecord {
        core: "iot",
        framework: "esp32-rust",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_DEPGATE,
    },
    FrameworkRecord {
        core: "iot",
        framework: "platformio",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_IOT_PIO,
    },
    FrameworkRecord {
        core: "iot",
        framework: "zephyr-arm",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_IOT_WEST,
    },
    FrameworkRecord {
        core: "iot",
        framework: "esp32",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "board variant (framework resolved at detect time from markers)",
    },
    FrameworkRecord {
        core: "iot",
        framework: "esp32c3",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "board variant (framework resolved at detect time from markers)",
    },
    FrameworkRecord {
        core: "iot",
        framework: "esp32dev",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "board variant (framework resolved at detect time from markers)",
    },
    FrameworkRecord {
        core: "iot",
        framework: "esp32s3",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "board variant (framework resolved at detect time from markers)",
    },
    FrameworkRecord {
        core: "iot",
        framework: "nodemcu-32s",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "board variant (framework resolved at detect time from markers)",
    },
    FrameworkRecord {
        core: "iot",
        framework: "nrf52dk_nrf52832",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "board variant (framework resolved at detect time from markers)",
    },
    FrameworkRecord {
        core: "iot",
        framework: "stm32f4_disc",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "board variant (framework resolved at detect time from markers)",
    },
    // ── lib: operation ownership is emitted from the firewall per language ──
    FrameworkRecord {
        core: "lib",
        framework: "ts",
        status: FrameworkStatus::NativeEngine,
        evidence: EV_LIB_PIPELINE,
    },
    FrameworkRecord {
        core: "lib",
        framework: "rust",
        status: FrameworkStatus::NativeEngine,
        evidence: "Rust resolver/CAS install path has local E2E; Cargo.lock synchronization for MGC-native add/update is not yet proven, so this record does not claim production-ready parity",
    },
    FrameworkRecord {
        core: "lib",
        framework: "python",
        status: FrameworkStatus::NativeEngine,
        evidence: "Python native path is manifest/artifact constrained; unsupported source/wheel/marker cases must fail closed; per-operation claims are emitted from dep_gate",
    },
    // ── web: base engine native; every named framework scaffold-only ──
    FrameworkRecord {
        core: "web",
        framework: "vanilla",
        status: FrameworkStatus::NativeEngine,
        evidence: EV_WEB_ENGINE,
    },
    FrameworkRecord {
        core: "web",
        framework: "ts",
        status: FrameworkStatus::NativeEngine,
        evidence: EV_WEB_ENGINE,
    },
    FrameworkRecord {
        core: "web",
        framework: "node",
        status: FrameworkStatus::NativeEngine,
        evidence: "wizard language branch onto the native js engine",
    },
    FrameworkRecord {
        core: "web",
        framework: "actix-web",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "angular",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "astro",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "axum",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "backend",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard branch node (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "custom",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "django",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "dotnet",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard language branch (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "dotnet-minimal",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "dotnet-webapi",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "echo",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "express",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "fastapi",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "fastify",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "fiber",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "flask",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "frontend",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard branch node (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "fullstack",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard branch node (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "gin",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_BACKEND,
    },
    FrameworkRecord {
        core: "web",
        framework: "go",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard language branch (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "hono",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "java",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard language branch (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "laravel",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "monorepo",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard branch node (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "nestjs",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "nextjs",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "nuxt",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "php",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard language branch (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "python",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard language branch (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "quarkus",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "qwik",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "react-fastify",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "react-spring",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "react-vite",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "remix",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "rust",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "wizard language branch (lifecycle unqualified)",
    },
    FrameworkRecord {
        core: "web",
        framework: "solidjs",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "spring-boot",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "sveltekit",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "symfony",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "trpc",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
    FrameworkRecord {
        core: "web",
        framework: "vue-laravel",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "vue-vite",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_E2E_WEB_JS,
    },
];

/// Machine string for the framework route, not dependency ownership.
/// (Chuỗi máy-đọc cho framework route, không phải quyền dependency.)
pub fn status_string(status: &FrameworkStatus) -> String {
    match status {
        FrameworkStatus::ScaffoldOnly => "scaffold-only".to_string(),
        FrameworkStatus::NativeEngine => "mgc-engine-path".to_string(),
    }
}

fn dependency_ecosystem(record: &FrameworkRecord) -> Option<&'static str> {
    use crate::commands::dep_gate::eco;
    match (record.core, record.framework) {
        ("ai", "python-agent") | ("lib", "python") => Some(eco::PYTHON),
        ("app", "flutter") => Some(eco::FLUTTER),
        ("app", "kotlin") => Some(eco::KOTLIN),
        ("app", "swift") => Some(eco::SWIFT),
        ("app", "react-native") => Some(eco::RN),
        ("app", "objc") => Some(eco::OBJC),
        ("lib", "ts") => Some(eco::TS),
        ("lib", "rust") => Some(eco::RUST),
        ("lib", "go") => Some(eco::GO),
        ("lib", "java") => Some(eco::JAVA),
        ("lib", "dotnet") => Some(eco::DOTNET),
        ("game", "bevy") => Some(eco::BEVY),
        ("iot", "esp32-rust") => Some("esp32-rust"),
        ("iot", "platformio") => Some("platformio"),
        ("iot", "zephyr-arm") => Some("zephyr"),
        ("clo", "terraform") => Some(eco::TERRAFORM),
        ("clo", "cdk" | "pulumi") => Some(eco::JS),
        (
            "web",
            "angular" | "astro" | "express" | "fastify" | "hono" | "nestjs" | "nextjs" | "nuxt"
            | "qwik" | "react-spring" | "react-vite" | "remix" | "solidjs" | "sveltekit" | "trpc"
            | "vanilla" | "vue-vite" | "node" | "ts",
        ) => Some(eco::JS),
        _ => None,
    }
}

fn dependency_ownership_json(record: &FrameworkRecord) -> serde_json::Value {
    use crate::commands::dep_gate::{DepContext, DepOp, DepOwner, owner_for};
    let ecosystem = dependency_ecosystem(record);
    let mut operations = serde_json::Map::new();
    for op in DepOp::ALL {
        let context = DepContext::new(record.core, ecosystem, Some(record.framework), None, *op);
        let owner = match owner_for(&context) {
            DepOwner::Native => "mgc-native",
            DepOwner::ScaffoldOnly => "scaffold-only",
            DepOwner::Unsupported => "unsupported",
        };
        let mut cell = serde_json::json!({"owner": owner});
        if matches!((record.core, record.framework), ("clo", "cdk" | "pulumi")) {
            cell["requires"] = serde_json::json!("package.json; embedded MGC JavaScript engine");
        }
        operations.insert(op.as_str().to_string(), cell);
    }
    serde_json::Value::Object(operations)
}

/// All records for one core, as machine-readable JSON values (consumed by
/// `mgc capabilities` — the registry is live data, not a dead table).
/// (Mọi record một core dạng JSON.)
pub fn qualification_json(core: &str) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = RECORDS
        .iter()
        .filter(|record| record.core == core)
        .map(|record| {
            serde_json::json!({
                "framework": record.framework,
                "status": status_string(&record.status),
                "dependency_ownership": dependency_ownership_json(record),
                "evidence": record.evidence,
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

#[cfg(test)]
#[path = "test/framework_records_test.rs"]
mod tests;
