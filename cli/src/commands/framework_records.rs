//! Framework qualification records — every wizard-selectable framework
//! id gets exactly one record stating what MagiCore may honestly claim
//! about it. A wizard answer is SCAFFOLD selection, never lifecycle
//! qualification: no record here may claim `Native` unless the lane runs
//! the full native pipeline, and `Delegated` names the exact compat tool.
//! A coverage test below parses `cli/src/wizard/*.rs` and fails when any
//! `Answer::new(.., "<id>")` lacks a record — new wizard entries cannot
//! silently arrive unqualified.
//!
//! (Hồ sơ qualification framework — mỗi id wizard chọn được có đúng một
//! record nêu điều MagiCore được phép claim trung thực. Test coverage
//! parse source wizard và fail khi id nào thiếu record.)

/// Honest lifecycle claim for one wizard framework choice.
/// (Claim lifecycle trung thực cho một lựa chọn framework.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameworkStatus {
    /// Offered by the wizard; lifecycle unqualified beyond scaffolding —
    /// no native/delegated/E2E claim.
    ScaffoldOnly,
    /// A compat runner implements the lifecycle under explicit opt-in.
    Delegated { tool: &'static str },
    /// Rides a native engine pipeline (dep_gate Native cell).
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

/// Every wizard-selectable framework id, exactly once per core.
/// (Mọi id framework wizard chọn được, đúng một lần mỗi core.)
pub const RECORDS: &[FrameworkRecord] = &[
    // ── ai ──
    FrameworkRecord {
        core: "ai",
        framework: "python-agent",
        status: FrameworkStatus::Delegated { tool: "uv/pip" },
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
        status: FrameworkStatus::Delegated { tool: "flutter" },
        evidence: EV_DEPGATE,
    },
    FrameworkRecord {
        core: "app",
        framework: "kotlin",
        status: FrameworkStatus::Delegated { tool: "gradle" },
        evidence: "dep_gate app/kotlin install+list cells + canary (add/remove/update: no runner)",
    },
    FrameworkRecord {
        core: "app",
        framework: "swift",
        status: FrameworkStatus::Delegated { tool: "swift" },
        evidence: "dep_gate app/swift install+list cells + canary (add/remove/update: no runner)",
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
        status: FrameworkStatus::Delegated { tool: "terraform" },
        evidence: "dep_gate clo/terraform install cell (add/remove/update: no runner)",
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
        status: FrameworkStatus::Delegated { tool: "cargo" },
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
        evidence: "optimizer/bench template materialize + list (dep_gate ScaffoldOnly)",
    },
    FrameworkRecord {
        core: "hardware",
        framework: "optimizer",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: "optimizer template materialize (no package lifecycle)",
    },
    // ── iot ──
    FrameworkRecord {
        core: "iot",
        framework: "esp32-rust",
        status: FrameworkStatus::Delegated { tool: "cargo" },
        evidence: EV_DEPGATE,
    },
    FrameworkRecord {
        core: "iot",
        framework: "platformio",
        status: FrameworkStatus::Delegated { tool: "pio" },
        evidence: EV_DEPGATE,
    },
    FrameworkRecord {
        core: "iot",
        framework: "zephyr-arm",
        status: FrameworkStatus::Delegated { tool: "west" },
        evidence: "lane framework id zephyr; dep_gate iot cell",
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
    // ── lib: install pipeline native; edits delegated per language ──
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
        evidence: "install pipeline native; add/remove/update delegate to cargo (dep_gate)",
    },
    FrameworkRecord {
        core: "lib",
        framework: "python",
        status: FrameworkStatus::NativeEngine,
        evidence: "install pipeline native; add/remove/update delegate to pip (dep_gate)",
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
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "angular",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "astro",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "axum",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
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
        evidence: EV_SCAFFOLD_WEB,
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
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "express",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "fastapi",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "fastify",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "fiber",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
    },
    FrameworkRecord {
        core: "web",
        framework: "flask",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_SCAFFOLD_WEB,
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
        evidence: EV_SCAFFOLD_WEB,
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
        evidence: EV_WIZARD,
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
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "nextjs",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "nuxt",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
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
        evidence: EV_WIZARD,
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
        evidence: EV_WIZARD,
    },
    FrameworkRecord {
        core: "web",
        framework: "remix",
        status: FrameworkStatus::ScaffoldOnly,
        evidence: EV_WIZARD,
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
        evidence: EV_WIZARD,
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
        evidence: EV_WIZARD,
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
        evidence: EV_WIZARD,
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
        evidence: EV_WIZARD,
    },
];

/// Machine string for one status (`delegated` carries its exact tool).
/// (Chuỗi máy-đọc cho status.)
pub fn status_string(status: &FrameworkStatus) -> String {
    match status {
        FrameworkStatus::ScaffoldOnly => "scaffold-only".to_string(),
        FrameworkStatus::Delegated { tool } => format!("delegated:{tool}"),
        FrameworkStatus::NativeEngine => "native-engine".to_string(),
    }
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
                "evidence": record.evidence,
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

#[cfg(test)]
mod tests {
    //! Coverage: every wizard answer id must have exactly one record.
    //! (Mọi id wizard phải có đúng một record.)

    use super::*;
    use std::collections::{HashMap, HashSet};

    /// Wizard source file → record core.
    const WIZARD_CORES: &[(&str, &str)] = &[
        ("ai.rs", "ai"),
        ("app.rs", "app"),
        ("cicd.rs", "cicd"),
        ("cloud.rs", "clo"),
        ("game.rs", "game"),
        ("hardware.rs", "hardware"),
        ("iot.rs", "iot"),
        ("lib.rs", "lib"),
        ("web.rs", "web"),
    ];

    /// Extract every `Answer::new("<display>", "<id>")` id from one wizard
    /// source file (plain scanner, no regex dependency).
    /// (Trích mọi id Answer::new từ một file wizard.)
    fn wizard_ids(source: &str) -> Vec<String> {
        let mut ids = Vec::new();
        let mut rest = source;
        while let Some(start) = rest.find("Answer::new(") {
            rest = &rest[start + "Answer::new(".len()..];
            // Skip the display string "...", then read the id string.
            let Some(first) = rest.find('"') else { break };
            let after_first = &rest[first + 1..];
            let Some(first_end) = after_first.find('"') else { break };
            let after_display = &after_first[first_end + 1..];
            let Some(second) = after_display.find('"') else { break };
            let after_second = &after_display[second + 1..];
            let Some(second_end) = after_second.find('"') else { break };
            ids.push(after_second[..second_end].to_string());
            rest = &after_second[second_end + 1..];
        }
        ids
    }

    #[test]
    fn every_wizard_answer_has_exactly_one_record() {
        let wizard_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/wizard");
        let mut missing = Vec::new();
        let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
        for record in RECORDS {
            *counts.entry((record.core, record.framework)).or_insert(0) += 1;
        }
        for (file, core) in WIZARD_CORES {
            let source = std::fs::read_to_string(wizard_dir.join(file))
                .unwrap_or_else(|_| panic!("wizard source missing: {file}"));
            for id in wizard_ids(&source) {
                match counts.get(&(core, id.as_str())) {
                    Some(1) => {}
                    Some(n) => panic!("duplicate records for {core}/{id}: {n}"),
                    None => missing.push(format!("{core}/{id}")),
                }
            }
        }
        assert!(
            missing.is_empty(),
            "wizard ids without qualification records: {missing:?}"
        );
    }

    #[test]
    fn no_record_claims_native_lifecycle_for_frameworks() {
        // The Development Preview verdict: named frameworks are
        // scaffold-only or delegated — `NativeEngine` is reserved for base
        // engine branches (vanilla/ts/node, lib ts/rust/python, cdk/pulumi).
        // (Verdict Preview: framework tên tuổi chỉ scaffold-only/delegated.)
        const ENGINE_BRANCHES: &[(&str, &str)] = &[
            ("web", "vanilla"),
            ("web", "ts"),
            ("web", "node"),
            ("lib", "ts"),
            ("lib", "rust"),
            ("lib", "python"),
            ("clo", "cdk"),
            ("clo", "pulumi"),
        ];
        for record in RECORDS {
            if matches!(record.status, FrameworkStatus::NativeEngine) {
                assert!(
                    ENGINE_BRANCHES.contains(&(record.core, record.framework)),
                    "{}:{} claims NativeEngine without an engine branch",
                    record.core,
                    record.framework
                );
            }
        }
    }

    #[test]
    fn records_resolve_known_frameworks() {
        // Spot-check the table: named frameworks carry their honest
        // claim; unknown ids are simply absent (fail-closed consumers
        // treat absence as ScaffoldOnly-or-worse, never native).
        fn find(core: &str, framework: &str) -> Option<&'static FrameworkRecord> {
            RECORDS
                .iter()
                .find(|record| record.core == core && record.framework == framework)
        }
        let django = find("web", "django").expect("django record");
        assert!(matches!(django.status, FrameworkStatus::ScaffoldOnly));
        let flutter = find("app", "flutter").expect("flutter record");
        assert!(matches!(
            flutter.status,
            FrameworkStatus::Delegated { tool: "flutter" }
        ));
        let vanilla = find("web", "vanilla").expect("vanilla record");
        assert!(matches!(vanilla.status, FrameworkStatus::NativeEngine));
        assert!(find("web", "not-a-framework").is_none());
        assert!(find("unknown-core", "django").is_none());
    }

    #[test]
    fn delegated_records_name_real_tools() {
        // Every Delegated tool must exist in the dep_gate compat universe
        // (a record may not promise a tool the gate cannot open).
        // (Mọi tool delegated phải thuộc universe compat của gate.)
        let mut seen = HashSet::new();
        for record in RECORDS {
            if let FrameworkStatus::Delegated { tool } = record.status {
                for part in tool.split('/') {
                    if seen.insert((record.core, part)) {
                        assert!(
                            crate::commands::dep_gate::DEPENDENCY_COMPAT_TOOLS
                                .contains(&part),
                            "{}:{} delegates to '{part}' outside the gate universe",
                            record.core,
                            record.framework
                        );
                    }
                }
            }
        }
    }
}
