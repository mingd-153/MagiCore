//! Tests for the C0 ownership firewall (T0.3).
//! Every decision is locked: native proceeds, delegated needs compat,
//! unsupported always fails — in every mode.

use super::*;
use crate::commands::compat::CompatMode;

fn native() -> CompatMode {
    CompatMode::Native
}

fn explicit(tool: &str) -> CompatMode {
    CompatMode::Explicit(tool.to_string())
}

#[test]
fn web_is_native_in_every_mode() {
    for op in [
        DepOp::Install,
        DepOp::Add,
        DepOp::Remove,
        DepOp::Update,
        DepOp::List,
    ] {
        assert!(gate("web", op, None, &native(), None).is_ok());
        assert!(gate("web", op, None, &explicit("cargo"), None).is_ok());
    }
}

#[test]
fn lib_typescript_is_native_but_other_lib_languages_delegate() {
    assert!(matches!(
        owner_for("lib", Some("ts"), DepOp::Install),
        DepOwner::Native
    ));
    assert!(gate_full("lib", Some("ts"), DepOp::Install, None, &native(), None).is_ok());
    // add/remove/update delegate through the toolchain for non-TS lib —
    // but install is native (verified spawn-free) for every language.
    for language in [None, Some("rust"), Some("python"), Some("go")] {
        assert!(
            gate_full("lib", language, DepOp::Add, None, &native(), None).is_err(),
            "lib {language:?} add must fail closed without compat"
        );
        assert!(
            gate_full("lib", language, DepOp::Install, None, &native(), None).is_ok(),
            "lib {language:?} install is native (no toolchain spawn)"
        );
    }
}

#[test]
fn lib_list_is_native_manifest_read() {
    assert!(gate("lib", DepOp::List, None, &native(), None).is_ok());
}

#[test]
fn delegated_lane_fails_closed_without_compat() {
    for (core, op) in [
        ("ai", DepOp::Install),
        ("ai", DepOp::Add),
        ("app", DepOp::Install),
        ("game", DepOp::Add),
        ("iot", DepOp::Update),
        ("clo", DepOp::Install),
    ] {
        let err = gate(core, op, None, &native(), None).unwrap_err();
        assert!(
            err.to_string().contains("--compat-runtime"),
            "{core} {} must name the escape hatch: {err}",
            op.as_str()
        );
    }
}

#[test]
fn compat_with_owning_tool_opens_the_gate() {
    assert!(gate("ai", DepOp::Install, Some("uv"), &explicit("uv"), None).is_ok());
    assert!(gate("ai", DepOp::Add, Some("pip"), &explicit("pip"), None).is_ok());
    assert!(gate("game", DepOp::Install, None, &explicit("cargo"), None).is_ok());
    assert!(
        gate(
            "app",
            DepOp::List,
            Some("flutter"),
            &explicit("flutter"),
            None
        )
        .is_ok()
    );
}

#[test]
fn compat_with_wrong_tool_stays_closed() {
    let err = gate("ai", DepOp::Install, Some("uv"), &explicit("pip"), None).unwrap_err();
    assert!(err.to_string().contains("does not own"), "{err}");
    let err = gate("game", DepOp::Install, None, &explicit("uv"), None).unwrap_err();
    assert!(err.to_string().contains("does not own"), "{err}");
}

#[test]
fn unsupported_cells_fail_in_every_mode_including_compat() {
    for mode in [native(), explicit("cargo"), explicit("terraform")] {
        for (core, op) in [
            ("cicd", DepOp::Install),
            ("hardware", DepOp::Add),
            ("unknown-core", DepOp::Install),
        ] {
            assert!(
                gate(core, op, None, &mode, None).is_err(),
                "{core} {} must fail closed even under compat",
                op.as_str()
            );
        }
    }
}

#[test]
fn dep_flag_parsing_accepts_toolchain_rejects_unknown() {
    assert!(from_dep_flag(None).is_ok());
    assert!(from_dep_flag(Some("uv")).is_ok());
    assert!(from_dep_flag(Some("cargo")).is_ok());
    assert!(from_dep_flag(Some("terraform")).is_ok());
    assert!(from_dep_flag(Some("not-a-tool")).is_err());
}

#[test]
fn capabilities_json_carries_dep_gate_ownership() {
    // T0.4: `mgc capabilities --json` is the single machine-readable
    // source for the matrix cross-check — every core reports all five
    // dependency ops with {owner, tools}, plus per-language overrides
    // for splitting cores.
    use crate::commands::capabilities::dependency_ownership;
    for core in [
        "web", "lib", "ai", "app", "game", "iot", "clo", "cicd", "hardware",
    ] {
        let ownership = dependency_ownership(core);
        let operations = &ownership["operations"];
        for op in ["install", "add", "remove", "update", "list"] {
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
    assert_eq!(web["install"]["owner"], "mgc-native");
    let ai = &dependency_ownership("ai")["operations"];
    assert_eq!(ai["install"]["owner"], "delegated");
    let hardware = &dependency_ownership("hardware")["operations"];
    assert_eq!(hardware["install"]["owner"], "unsupported");
    // Splitting cores expose per-language overrides that differ from the
    // core-level branch (lib/ts native add, app/rn unsupported install).
    let lib_languages = dependency_ownership("lib")["languages"].clone();
    assert_eq!(lib_languages["ts"]["add"]["owner"], "mgc-native");
    let app_languages = dependency_ownership("app")["languages"].clone();
    assert_eq!(app_languages["rn"]["install"]["owner"], "unsupported");
}
