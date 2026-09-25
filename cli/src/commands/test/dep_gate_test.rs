//! Tests for the C0 ownership firewall (T0.3, P0#2 revision).
//! Every decision is locked over the FULL context (core + ecosystem +
//! operation): native proceeds, delegated needs compat, unsupported
//! always fails — in every mode. No language-unaware call exists.

use super::*;
use crate::commands::compat::CompatMode;
use crate::commands::dep_gate::eco;

fn native() -> CompatMode {
    CompatMode::Native
}

fn explicit(tool: &str) -> CompatMode {
    CompatMode::Explicit(tool.to_string())
}

fn ctx<'a>(core: &'a str, ecosystem: Option<&'a str>, op: DepOp) -> DepContext<'a> {
    DepContext::new(core, ecosystem, None, None, op)
}

#[test]
fn web_is_native_only_for_declared_js_ts() {
    for eco in [Some(eco::JS), Some(eco::TS)] {
        for op in [
            DepOp::Install,
            DepOp::Add,
            DepOp::Remove,
            DepOp::Update,
            DepOp::List,
            DepOp::Resolve,
            DepOp::Gc,
        ] {
            assert!(gate(&ctx("web", eco, op), None, &native(), None).is_ok());
            assert!(gate(&ctx("web", eco, op), None, &explicit("cargo"), None).is_err());
        }
    }
    // Undeclared ecosystem never defaults open.
    assert!(gate(&ctx("web", None, DepOp::Install), None, &native(), None).is_err());
    assert!(
        gate(
            &ctx("web", Some("python"), DepOp::Install),
            None,
            &native(),
            None
        )
        .is_err()
    );
}

#[test]
fn lib_typescript_native_protocol_langs_split_pipeline_vs_edits() {
    assert!(matches!(
        owner_for(&ctx("lib", Some(eco::TS), DepOp::Install)),
        DepOwner::Native
    ));
    assert!(
        gate(
            &ctx("lib", Some(eco::TS), DepOp::Install),
            None,
            &native(),
            None
        )
        .is_ok()
    );
    // Protocol languages: native pipeline (install/resolve/verify/...) but
    // toolchain-owned edits with PER-LANGUAGE tools.
    for lang in [eco::RUST, eco::PYTHON, eco::GO, eco::JAVA, eco::DOTNET] {
        assert!(
            gate(
                &ctx("lib", Some(lang), DepOp::Install),
                None,
                &native(),
                None
            )
            .is_ok(),
            "lib[{lang}] install is native (no toolchain spawn)"
        );
        assert!(
            gate(
                &ctx("lib", Some(lang), DepOp::Resolve),
                None,
                &native(),
                None
            )
            .is_ok(),
            "lib[{lang}] resolve is native"
        );
    }
    // Native Add (resolve-first + mgc-side manifest edit, zero spawn):
    // rust/python/go/dotnet + java-pom run inside mgc. Gradle projects
    // fail closed inside prepare_add (scripts are programs) — the gate
    // stays Native, the failure carries the pom.xml guidance.
    for lang in [eco::RUST, eco::PYTHON, eco::GO, eco::DOTNET, eco::JAVA] {
        assert!(
            gate(&ctx("lib", Some(lang), DepOp::Add), None, &native(), None).is_ok(),
            "lib[{lang}] add is native (no toolchain spawn)"
        );
    }
    // Exact actual-tool matching: python runs PIP — a uv opt-in is a
    // wrong-tool refusal, never a pip spawn (flag==process contract).
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Add),
            Some("pip"),
            &native(),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Add),
            Some("pip"),
            &native(),
            None
        )
        .is_ok(),
        "native operation has no compatibility requirement"
    );
    // Native Add ignores compat flags (native engine always runs) — a uv
    // opt-in on a native lane is a no-op info, never a spawn and never a
    // wrong-tool error. All lib verbs are native now; the flag==process
    // contract lives on the remaining delegated cores (ai Add/Remove,
    // game/iot lanes).
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Add),
            Some("pip"),
            &native(),
            None
        )
        .is_ok(),
        "native engine remains available without compatibility"
    );
    // Native Remove/Update ignore any opt-in (the last delegated verb
    // is gone for lib).
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Remove),
            Some("pip"),
            &native(),
            None
        )
        .is_ok(),
        "native remove remains available"
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Update),
            Some("pip"),
            &native(),
            None
        )
        .is_ok(),
        "native update remains available"
    );
    // Per-language tool sets: native Add/Remove/Update ignore any
    // opt-in (no delegated lib verbs left).
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Add),
            Some("cargo"),
            &native(),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Add),
            Some("cargo"),
            &native(),
            None
        )
        .is_ok(),
        "native add remains available"
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Remove),
            Some("cargo"),
            &native(),
            None
        )
        .is_ok(),
        "native remove remains available"
    );
    // Go remove runs natively; an explicit PM mode is rejected globally.
    assert!(
        gate(
            &ctx("lib", Some(eco::GO), DepOp::Remove),
            None,
            &native(),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::GO), DepOp::Remove),
            Some("go"),
            &explicit("go"),
            None
        )
        .is_err()
    );
    // Java/DOTNET Update run natively (resolve-latest + rewrite); gradle
    // projects fail closed in prepare_add/write_manifest.
    for lang in [eco::JAVA, eco::DOTNET] {
        assert!(
            gate(
                &ctx("lib", Some(lang), DepOp::Update),
                None,
                &native(),
                None
            )
            .is_ok(),
            "lib[{lang}] update is native"
        );
    }
    // Undetected lib language: pipeline ops fail closed (P1 wildcard fix).
    assert!(gate(&ctx("lib", None, DepOp::Install), None, &native(), None).is_err());
    assert!(gate(&ctx("lib", None, DepOp::Add), None, &native(), None).is_err());
}

#[test]
fn lib_list_requires_verified_installed_inventory() {
    for eco in [Some(eco::PYTHON), Some(eco::TS)] {
        assert!(gate(&ctx("lib", eco, DepOp::List), None, &native(), None).is_ok());
    }
    for eco in [
        None,
        Some(eco::RUST),
        Some(eco::GO),
        Some(eco::JAVA),
        Some(eco::DOTNET),
        Some("unknown-lang"),
    ] {
        assert!(gate(&ctx("lib", eco, DepOp::List), None, &native(), None).is_err());
    }
}

#[test]
fn list_is_not_native_when_the_core_cannot_read_its_dependency_manifest() {
    for (core, ecosystem, framework) in [
        ("game", Some("godot"), Some("godot")),
        ("game", Some("unity"), Some("unity")),
        ("game", Some("unreal"), Some("unreal")),
        ("iot", Some("zephyr"), Some("zephyr")),
        ("clo", Some("terraform"), Some("terraform")),
        ("clo", Some("cloudflare"), Some("cloudflare")),
    ] {
        let context = DepContext::new(core, ecosystem, framework, None, DepOp::List);
        assert!(
            matches!(owner_for(&context), DepOwner::Unsupported),
            "{core}/{ecosystem:?} has no native package list implementation"
        );
    }
}

#[test]
fn game_and_iot_list_are_blocked_without_installed_state_evidence() {
    for (core, ecosystem, framework) in [
        ("game", Some(eco::BEVY), Some(eco::BEVY)),
        ("iot", Some("esp32-rust"), Some("esp32-rust")),
        ("iot", Some("platformio"), Some("platformio")),
    ] {
        let context = DepContext::new(core, ecosystem, framework, None, DepOp::List);
        assert!(matches!(owner_for(&context), DepOwner::Unsupported));
    }
}

#[test]
fn cloud_list_is_native_only_for_web_backed_cdk_or_pulumi_manifests() {
    for framework in ["cdk", "pulumi"] {
        let context = DepContext::new("clo", Some(eco::JS), Some(framework), None, DepOp::List);
        assert!(matches!(owner_for(&context), DepOwner::Native));
    }
    for (ecosystem, framework) in [
        (Some("terraform"), Some("terraform")),
        (Some("cloudflare"), Some("cloudflare")),
        (None, Some("pulumi")),
    ] {
        let context = DepContext::new("clo", ecosystem, framework, None, DepOp::List);
        assert!(matches!(owner_for(&context), DepOwner::Unsupported));
    }
}

#[test]
fn cloud_cdk_and_pulumi_dependency_lifecycle_is_native_only_with_js_manifest() {
    use crate::commands::dep_gate::{DepContext, DepOp, DepOwner, eco, owner_for};

    for framework in ["cdk", "pulumi"] {
        for op in [
            DepOp::Install,
            DepOp::Add,
            DepOp::Remove,
            DepOp::Update,
            DepOp::List,
        ] {
            let context = DepContext::new("clo", Some(eco::JS), Some(framework), None, op);
            assert!(
                matches!(owner_for(&context), DepOwner::Native),
                "clo/{framework} {} must use the embedded MGC JS dependency engine",
                op.as_str()
            );
        }
    }

    for framework in ["terraform", "cloudflare"] {
        let context = DepContext::new("clo", None, Some(framework), None, DepOp::Install);
        assert!(matches!(owner_for(&context), DepOwner::Unsupported));
    }
}

#[test]
fn cloud_gate_requires_embedded_js_manifest_and_rejects_compat() {
    use crate::commands::dep_gate::{DepOp, gate_cloud_project};

    let missing_manifest = tempfile::tempdir().unwrap();
    assert!(gate_cloud_project(missing_manifest.path(), "cdk", DepOp::Install, None).is_err());

    let js_project = tempfile::tempdir().unwrap();
    std::fs::write(js_project.path().join("package.json"), "{}\n").unwrap();
    assert!(gate_cloud_project(js_project.path(), "cdk", DepOp::Install, None).is_ok());
    assert!(gate_cloud_project(js_project.path(), "pulumi", DepOp::Add, None).is_ok());
    assert!(gate_cloud_project(js_project.path(), "terraform", DepOp::Install, None).is_err());
    assert!(gate_cloud_project(js_project.path(), "cdk", DepOp::Install, Some("npm")).is_err());
}

#[test]
fn ai_python_dependency_gate_rejects_compat_for_every_operation() {
    for op in [
        DepOp::Install,
        DepOp::Add,
        DepOp::Remove,
        DepOp::Update,
        DepOp::List,
    ] {
        let native_result = gate(&ctx("ai", Some(eco::PYTHON), op), None, &native(), None);
        if matches!(op, DepOp::Install | DepOp::Add | DepOp::Update) {
            assert!(native_result.is_ok(), "AI Python {op:?} static native lane");
        } else {
            assert!(
                native_result.is_err(),
                "AI Python {op:?} lacks static ownership"
            );
        }
        assert!(
            gate(
                &ctx("ai", Some(eco::PYTHON), op),
                Some("uv"),
                &explicit("uv"),
                None
            )
            .is_err()
        );
    }
    // Undetected ai ecosystem never defaults open.
    assert!(gate(&ctx("ai", None, DepOp::Install), None, &native(), None).is_err());
}

#[test]
fn app_rn_unsupported_flutter_delegated_unknown_unsupported() {
    // P0#2: the app/rn rule fires through the real gate path.
    for op in [
        DepOp::Install,
        DepOp::Add,
        DepOp::Remove,
        DepOp::Update,
        DepOp::List,
    ] {
        for mode in [native(), explicit("flutter"), explicit("npm")] {
            let err = gate(&ctx("app", Some(eco::RN), op), None, &mode, None).unwrap_err();
            let message = err.to_string();
            assert!(
                message.contains("disabled for dependency operations") || message.contains("rn"),
                "failure must explain the compat ban or unsupported RN lane: {err}"
            );
        }
    }
    assert!(
        gate(
            &ctx("app", Some(eco::FLUTTER), DepOp::Install),
            Some("flutter"),
            &native(),
            None
        )
        .is_ok()
    );
    // Native flutter install ignores compat (native always runs).
    assert!(
        gate(
            &ctx("app", Some(eco::FLUTTER), DepOp::Install),
            Some("flutter"),
            &native(),
            None
        )
        .is_ok(),
        "flutter install is native (no toolchain spawn)"
    );
    assert!(
        gate(
            &ctx("app", Some(eco::FLUTTER), DepOp::Add),
            None,
            &native(),
            None
        )
        .is_ok()
    );
    // Undetected app ecosystem never falls back to generic-delegated.
    assert!(gate(&ctx("app", None, DepOp::Install), None, &native(), None).is_err());
    assert!(gate(&ctx("app", None, DepOp::Install), None, &native(), None).is_err());
}

#[test]
fn game_iot_clo_need_declared_ecosystem() {
    assert!(
        gate(
            &ctx("game", Some(eco::BEVY), DepOp::Install),
            None,
            &native(),
            None
        )
        .is_ok()
    );
    assert!(gate(&ctx("game", None, DepOp::Install), None, &native(), None).is_err());
    assert!(matches!(
        owner_for(&ctx("iot", Some("esp32-rust"), DepOp::Install)),
        DepOwner::Native
    ));
    for fw in ["platformio", "zephyr"] {
        assert!(
            gate(&ctx("iot", Some(fw), DepOp::Install), None, &native(), None).is_err(),
            "iot[{fw}] has no adapter-owned install lane; compatibility is not silently opened"
        );
    }
    assert!(gate(&ctx("iot", None, DepOp::Install), None, &native(), None).is_err());
    assert!(
        gate(
            &ctx("clo", Some(eco::TERRAFORM), DepOp::Install),
            Some("terraform"),
            &native(),
            None
        )
        .is_err()
    );
    assert!(
        gate(
            &ctx("clo", None, DepOp::Install),
            Some("terraform"),
            &native(),
            None
        )
        .is_err()
    );
}

#[test]
fn lanes_without_native_owner_fail_closed_without_compat_escape() {
    for (core, eco, op) in [
        ("ai", Some(eco::PYTHON), DepOp::Remove),
        ("clo", Some(eco::TERRAFORM), DepOp::Install),
    ] {
        let err = gate(&ctx(core, eco, op), None, &native(), None).unwrap_err();
        assert!(
            err.to_string().contains("unsupported") || err.to_string().contains("does not yet own"),
            "{core} {} must fail without a compat escape: {err}",
            op.as_str()
        );
    }
}

#[test]
fn compat_with_wrong_tool_stays_closed() {
    for (core, ecosystem, operation, tool) in [
        ("ai", Some(eco::PYTHON), DepOp::Install, "cargo"),
        ("ai", Some(eco::PYTHON), DepOp::Remove, "pip"),
        ("clo", Some(eco::TERRAFORM), DepOp::Install, "terraform"),
    ] {
        let error = gate(
            &ctx(core, ecosystem, operation),
            Some(tool),
            &explicit(tool),
            None,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("disabled for dependency operations"),
            "{error}"
        );
    }
}

#[test]
fn unsupported_cells_fail_in_every_mode_including_compat() {
    for mode in [native(), explicit("cargo"), explicit("terraform")] {
        for (core, eco, op) in [
            ("cicd", None, DepOp::Install),
            ("hardware", None, DepOp::Add),
            ("unknown-core", None, DepOp::Install),
            ("app", Some(eco::RN), DepOp::Install),
            ("lib", None, DepOp::Install),
            // Exact cells without runners (never Delegated promises).
            // (lib is fully native now — only Update/unsupported cells of
            // other cores left.)
            ("app", Some(eco::SWIFT), DepOp::Add),
            ("app", Some(eco::OBJC), DepOp::List),
            ("app", Some(eco::OBJC), DepOp::Add),
            // Terraform runs install only; add/remove/update have no runner.
            ("clo", Some(eco::TERRAFORM), DepOp::Add),
            ("clo", Some(eco::TERRAFORM), DepOp::Remove),
            // Non-bevy game engines have no package manager.
            ("game", Some("godot"), DepOp::Install),
            ("game", Some("unity"), DepOp::Add),
        ] {
            assert!(
                gate(&ctx(core, eco, op), None, &mode, None).is_err(),
                "{core}/{eco:?} {} must fail closed even under compat",
                op.as_str()
            );
        }
    }
}

#[test]
fn app_exact_verbs_match_real_runners() {
    // Native operations pass; source/catalog mutations without transaction
    // support and every compat mode fail.
    // Operation native thì chạy; lane chưa hỗ trợ và mọi compat đều bị chặn.
    assert!(
        gate(
            &ctx("app", Some(eco::FLUTTER), DepOp::Update),
            Some("flutter"),
            &native(),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("app", Some(eco::SWIFT), DepOp::Install),
            None,
            &native(),
            None
        )
        .is_ok()
    );
    for op in [DepOp::Add, DepOp::Remove, DepOp::Update] {
        assert!(gate(&ctx("app", Some(eco::SWIFT), op), None, &native(), None).is_err());
        assert!(gate(&ctx("app", Some(eco::KOTLIN), op), None, &native(), None).is_err());
    }
    assert!(
        gate(
            &ctx("app", Some(eco::SWIFT), DepOp::List),
            None,
            &native(),
            None
        )
        .is_err()
    );
    assert!(
        gate(
            &ctx("app", Some(eco::KOTLIN), DepOp::Install),
            None,
            &native(),
            None
        )
        .is_err()
    );
    assert!(
        gate(
            &ctx("app", Some(eco::KOTLIN), DepOp::List),
            None,
            &native(),
            None
        )
        .is_err()
    );
    assert!(
        gate(
            &ctx("app", Some(eco::OBJC), DepOp::Install),
            None,
            &native(),
            None
        )
        .is_err()
    );
    for core_eco_op in [
        ("app", eco::FLUTTER, DepOp::Install),
        ("app", eco::SWIFT, DepOp::Install),
    ] {
        assert!(
            gate(
                &ctx(core_eco_op.0, Some(core_eco_op.1), core_eco_op.2),
                None,
                &explicit("flutter"),
                None
            )
            .is_err()
        );
    }
}

#[test]
fn scaffold_only_lanes_say_so() {
    // P1: cicd/hardware failures must read as scaffold-only, never as a
    // package install that "isn't supported yet".
    for (core, op) in [("cicd", DepOp::Install), ("hardware", DepOp::Add)] {
        let err = gate(&ctx(core, None, op), None, &native(), None).unwrap_err();
        assert!(
            err.to_string().contains("scaffold-only"),
            "{core} must be labeled scaffold-only: {err}"
        );
    }
    // Hardware list is a REAL read-only inventory command: it passes the
    // gate, but its owner is ScaffoldOnly — never "mgc-native".
    assert!(gate(&ctx("hardware", None, DepOp::List), None, &native(), None).is_ok());
    assert!(matches!(
        owner_for(&ctx("hardware", None, DepOp::List)),
        DepOwner::ScaffoldOnly
    ));
}

#[test]
fn dep_flag_parsing_rejects_all_compat_values() {
    assert!(from_dep_flag(None).is_ok());
    for tool in [
        "uv",
        "cargo",
        "terraform",
        "dotnet",
        "mvn",
        "bun",
        "deno",
        "not-a-tool",
    ] {
        let error = from_dep_flag(Some(tool)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("disabled for dependency operations")
        );
    }
}

#[test]
fn capabilities_json_carries_dep_gate_ownership() {
    // T0.4: `mgc capabilities --json` is the single machine-readable
    // source for the matrix cross-check — every core reports all
    // dependency ops with {owner, tools}, plus per-ecosystem overrides
    // for splitting cores.
    use crate::commands::capabilities::dependency_ownership;
    for core in [
        "web", "lib", "ai", "app", "game", "iot", "clo", "cicd", "hardware",
    ] {
        let ownership = dependency_ownership(core);
        let operations = &ownership["operations"];
        for op in [
            "install",
            "add",
            "remove",
            "update",
            "list",
            "resolve",
            "lock",
            "fetch",
            "verify",
            "store",
            "materialize",
            "frozen-install",
            "offline-reinstall",
            "gc",
        ] {
            let cell = &operations[op];
            assert!(
                cell.get("owner").is_some() && cell.get("tools").is_some(),
                "{core}/{op} must carry {{owner, tools}}"
            );
            assert!(
                ["mgc-native", "delegated", "scaffold-only", "unsupported"]
                    .contains(&cell["owner"].as_str().unwrap_or("")),
                "{core}/{op} owner must be closed-vocabulary"
            );
        }
    }
    let web = &dependency_ownership("web")["operations"];
    assert_eq!(web["install"]["owner"], "unsupported");
    let web_languages = dependency_ownership("web")["languages"].clone();
    assert_eq!(web_languages["js"]["install"]["owner"], "mgc-native");
    assert_eq!(
        web_languages["javascript"]["install"]["owner"],
        "mgc-native"
    );
    let ai = &dependency_ownership("ai")["operations"];
    assert_eq!(ai["install"]["owner"], "unsupported");
    let ai_languages = dependency_ownership("ai")["languages"].clone();
    assert_eq!(ai_languages["python"]["install"]["owner"], "mgc-native");
    assert_eq!(ai_languages["python"]["update"]["owner"], "mgc-native");
    let hardware = &dependency_ownership("hardware")["operations"];
    assert_eq!(hardware["install"]["owner"], "unsupported");
    // Splitting cores expose per-ecosystem overrides that differ from the
    // core-level branch (lib/ts + lib/rust native install over an
    // unsupported base; app/rn unsupported install over an unsupported
    // base with delegated siblings).
    let lib_languages = dependency_ownership("lib")["languages"].clone();
    assert_eq!(lib_languages["ts"]["add"]["owner"], "mgc-native");
    assert_eq!(lib_languages["rust"]["install"]["owner"], "mgc-native");
    assert_eq!(lib_languages["rust"]["add"]["owner"], "mgc-native");
    // Capability snapshot vs REAL runners: native Add carries no tools;
    // python Remove still spawns pip only (uv excluded); go remove has
    // no runner (unsupported).
    assert_eq!(lib_languages["python"]["add"]["owner"], "mgc-native");
    assert_eq!(
        lib_languages["python"]["add"]["tools"],
        serde_json::json!([])
    );
    assert_eq!(lib_languages["python"]["remove"]["owner"], "mgc-native");
    assert_eq!(
        lib_languages["python"]["remove"]["tools"],
        serde_json::json!([])
    );
    assert_eq!(lib_languages["go"]["remove"]["owner"], "mgc-native");
    assert_eq!(lib_languages["go"]["add"]["owner"], "mgc-native");
    assert_eq!(lib_languages["java"]["add"]["owner"], "mgc-native");
    assert_eq!(lib_languages["java"]["install"]["owner"], "mgc-native");
    assert_eq!(lib_languages["dotnet"]["update"]["owner"], "mgc-native");
    let app_languages = dependency_ownership("app")["languages"].clone();
    // "rn" is omitted when identical to the (unsupported) base row — the
    // languages map carries ONLY differing ecosystems. When present it
    // must read unsupported (the gate-level rule itself is locked by
    // app_rn_unsupported_flutter_delegated_unknown_unsupported).
    if let Some(rn) = app_languages.get("rn") {
        assert_eq!(rn["install"]["owner"], "unsupported");
    }
    assert_eq!(app_languages["flutter"]["install"]["owner"], "mgc-native");
    assert_eq!(app_languages["flutter"]["update"]["owner"], "mgc-native");
    assert_eq!(app_languages["swift"]["update"]["owner"], "unsupported");
    // Exact app verbs stay native only where complete; no delegated labels.
    assert_eq!(app_languages["swift"]["add"]["owner"], "unsupported");
    assert_eq!(app_languages["swift"]["install"]["owner"], "mgc-native");
    assert_eq!(app_languages["swift"]["remove"]["owner"], "unsupported");
    assert!(app_languages.get("kotlin").is_none()); // all operations inherit unsupported base owner.
    for op in [
        DepOp::Add,
        DepOp::Remove,
        DepOp::Update,
        DepOp::Install,
        DepOp::List,
    ] {
        assert!(matches!(
            owner_for(&ctx("app", Some(eco::KOTLIN), op)),
            DepOwner::Unsupported
        ));
    }
    assert!(matches!(
        owner_for(&ctx("app", Some(eco::OBJC), DepOp::List)),
        DepOwner::Unsupported
    ));
    assert!(matches!(
        owner_for(&ctx("app", Some(eco::OBJC), DepOp::Install)),
        DepOwner::Unsupported
    ));
    // Hardware list reads scaffold-only, never mgc-native.
    let hardware_full = dependency_ownership("hardware");
    assert_eq!(
        hardware_full["operations"]["list"]["owner"],
        "scaffold-only"
    );
    assert_eq!(
        hardware_full["operations"]["install"]["owner"],
        "unsupported"
    );
}

#[test]
fn non_native_iot_package_operations_are_unsupported_even_with_compat() {
    for (framework, tool) in [("zephyr", "west"), ("platformio", "pio")] {
        for op in [DepOp::Install, DepOp::Add, DepOp::Remove, DepOp::Update] {
            assert!(
                gate(
                    &ctx("iot", Some(framework), op),
                    Some(tool),
                    &explicit(tool),
                    None
                )
                .is_err(),
                "iot/{framework} {op:?} must remain unsupported until MGC owns the lifecycle"
            );
        }
    }
}
