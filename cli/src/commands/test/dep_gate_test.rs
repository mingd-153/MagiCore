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
            assert!(gate(&ctx("web", eco, op), None, &explicit("cargo"), None).is_ok());
        }
    }
    // Undeclared ecosystem never defaults open.
    assert!(gate(&ctx("web", None, DepOp::Install), None, &native(), None).is_err());
    assert!(gate(&ctx("web", Some("python"), DepOp::Install), None, &native(), None).is_err());
}

#[test]
fn lib_typescript_native_protocol_langs_split_pipeline_vs_edits() {
    assert!(matches!(
        owner_for(&ctx("lib", Some(eco::TS), DepOp::Install)),
        DepOwner::Native
    ));
    assert!(gate(&ctx("lib", Some(eco::TS), DepOp::Install), None, &native(), None).is_ok());
    // Protocol languages: native pipeline (install/resolve/verify/...) but
    // toolchain-owned edits (add/remove/update) with PER-LANGUAGE tools.
    for lang in [eco::RUST, eco::PYTHON, eco::GO, eco::JAVA, eco::DOTNET] {
        assert!(
            gate(&ctx("lib", Some(lang), DepOp::Install), None, &native(), None).is_ok(),
            "lib[{lang}] install is native (no toolchain spawn)"
        );
        assert!(
            gate(&ctx("lib", Some(lang), DepOp::Resolve), None, &native(), None).is_ok(),
            "lib[{lang}] resolve is native"
        );
        assert!(
            gate(&ctx("lib", Some(lang), DepOp::Add), None, &native(), None).is_err(),
            "lib[{lang}] add must fail closed without compat"
        );
    }
    // Per-language tool sets: a rust project cannot opt in with uv.
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Add),
            None,
            &explicit("cargo"),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Add),
            None,
            &explicit("uv"),
            None
        )
        .is_err()
    );
    // Undetected lib language: pipeline ops fail closed (P1 wildcard fix).
    assert!(gate(&ctx("lib", None, DepOp::Install), None, &native(), None).is_err());
    assert!(gate(&ctx("lib", None, DepOp::Add), None, &native(), None).is_err());
}

#[test]
fn lib_list_is_native_manifest_read_for_any_ecosystem() {
    for eco in [None, Some(eco::TS), Some(eco::RUST), Some("unknown-lang")] {
        assert!(gate(&ctx("lib", eco, DepOp::List), None, &native(), None).is_ok());
    }
}

#[test]
fn ai_python_delegated_unknown_ecosystem_unsupported() {
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Install),
            Some("uv"),
            &explicit("uv"),
            None
        )
        .is_ok()
    );
    // pip ~ pip3 alias: same owner, either opt-in opens the lane.
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Install),
            Some("pip"),
            &explicit("pip3"),
            None
        )
        .is_ok()
    );
    // uv vs pip still mismatches (different owners).
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Install),
            Some("uv"),
            &explicit("pip"),
            None
        )
        .is_err()
    );
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
            assert!(
                err.to_string().contains("rn"),
                "RN failure must name the ecosystem: {err}"
            );
        }
    }
    assert!(
        gate(
            &ctx("app", Some(eco::FLUTTER), DepOp::Install),
            Some("flutter"),
            &explicit("flutter"),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("app", Some(eco::FLUTTER), DepOp::Install),
            Some("flutter"),
            &native(),
            None
        )
        .is_err()
    );
    // Undetected app ecosystem never falls back to generic-delegated.
    assert!(gate(&ctx("app", None, DepOp::Install), None, &native(), None).is_err());
    assert!(
        gate(
            &ctx("app", None, DepOp::Install),
            None,
            &explicit("flutter"),
            None
        )
        .is_err()
    );
}

#[test]
fn game_iot_clo_need_declared_ecosystem() {
    assert!(
        gate(
            &ctx("game", Some(eco::BEVY), DepOp::Install),
            None,
            &explicit("cargo"),
            None
        )
        .is_ok()
    );
    assert!(gate(&ctx("game", None, DepOp::Install), None, &explicit("cargo"), None).is_err());
    for fw in ["esp32-rust", "platformio", "zephyr"] {
        assert!(
            gate(&ctx("iot", Some(fw), DepOp::Install), None, &explicit("pio"), None).is_ok(),
            "iot[{fw}] must open for its toolchain set"
        );
    }
    assert!(gate(&ctx("iot", None, DepOp::Install), None, &explicit("pio"), None).is_err());
    assert!(
        gate(
            &ctx("clo", Some(eco::TERRAFORM), DepOp::Install),
            Some("terraform"),
            &explicit("terraform"),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("clo", None, DepOp::Install),
            Some("terraform"),
            &explicit("terraform"),
            None
        )
        .is_err()
    );
}

#[test]
fn delegated_lane_fails_closed_without_compat() {
    for (core, eco, op) in [
        ("ai", Some(eco::PYTHON), DepOp::Install),
        ("ai", Some(eco::PYTHON), DepOp::Add),
        ("app", Some(eco::FLUTTER), DepOp::Install),
        ("game", Some(eco::BEVY), DepOp::Add),
        ("iot", Some("esp32-rust"), DepOp::Update),
        ("clo", Some(eco::TERRAFORM), DepOp::Install),
    ] {
        let err = gate(&ctx(core, eco, op), None, &native(), None).unwrap_err();
        assert!(
            err.to_string().contains("--compat-runtime"),
            "{core} {} must name the escape hatch: {err}",
            op.as_str()
        );
    }
}

#[test]
fn compat_with_wrong_tool_stays_closed() {
    let err = gate(
        &ctx("ai", Some(eco::PYTHON), DepOp::Install),
        Some("uv"),
        &explicit("cargo"),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("does not own"), "{err}");
    let err = gate(
        &ctx("game", Some(eco::BEVY), DepOp::Install),
        None,
        &explicit("uv"),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("does not own"), "{err}");
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
    // Hardware list is a spawn-free adapter read, so it stays native.
    assert!(gate(&ctx("hardware", None, DepOp::List), None, &native(), None).is_ok());
}

#[test]
fn dep_flag_parsing_accepts_toolchain_rejects_unknown() {
    assert!(from_dep_flag(None).is_ok());
    assert!(from_dep_flag(Some("uv")).is_ok());
    assert!(from_dep_flag(Some("cargo")).is_ok());
    assert!(from_dep_flag(Some("terraform")).is_ok());
    assert!(from_dep_flag(Some("dotnet")).is_ok());
    assert!(from_dep_flag(Some("mvn")).is_ok());
    assert!(from_dep_flag(Some("not-a-tool")).is_err());
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
                ["mgc-native", "delegated", "unsupported"]
                    .contains(&cell["owner"].as_str().unwrap_or("")),
                "{core}/{op} owner must be closed-vocabulary"
            );
        }
    }
    let web = &dependency_ownership("web")["operations"];
    assert_eq!(web["install"]["owner"], "unsupported");
    let web_languages = dependency_ownership("web")["languages"].clone();
    assert_eq!(web_languages["js"]["install"]["owner"], "mgc-native");
    assert_eq!(web_languages["javascript"]["install"]["owner"], "mgc-native");
    let ai = &dependency_ownership("ai")["operations"];
    assert_eq!(ai["install"]["owner"], "unsupported");
    let ai_languages = dependency_ownership("ai")["languages"].clone();
    assert_eq!(ai_languages["python"]["install"]["owner"], "delegated");
    let hardware = &dependency_ownership("hardware")["operations"];
    assert_eq!(hardware["install"]["owner"], "unsupported");
    // Splitting cores expose per-ecosystem overrides that differ from the
    // core-level branch (lib/ts + lib/rust native install over an
    // unsupported base; app/rn unsupported install over an unsupported
    // base with delegated siblings).
    let lib_languages = dependency_ownership("lib")["languages"].clone();
    assert_eq!(lib_languages["ts"]["add"]["owner"], "mgc-native");
    assert_eq!(lib_languages["rust"]["install"]["owner"], "mgc-native");
    assert_eq!(lib_languages["rust"]["add"]["owner"], "delegated");
    let app_languages = dependency_ownership("app")["languages"].clone();
    // "rn" is omitted when identical to the (unsupported) base row — the
    // languages map carries ONLY differing ecosystems. When present it
    // must read unsupported (the gate-level rule itself is locked by
    // app_rn_unsupported_flutter_delegated_unknown_unsupported).
    if let Some(rn) = app_languages.get("rn") {
        assert_eq!(rn["install"]["owner"], "unsupported");
    }
    assert_eq!(app_languages["flutter"]["install"]["owner"], "delegated");
}
