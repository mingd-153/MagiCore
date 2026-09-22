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
            &explicit("pip"),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Add),
            Some("pip"),
            &explicit("pip3"),
            None
        )
        .is_ok(),
        "pip~pip3 alias: same owner"
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
            &explicit("uv"),
            None
        )
        .is_ok(),
        "compat on native add is ignored (native always runs)"
    );
    // Native Remove/Update ignore any opt-in (the last delegated verb
    // is gone for lib).
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Remove),
            Some("pip"),
            &explicit("uv"),
            None
        )
        .is_ok(),
        "compat on native remove is ignored"
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::PYTHON), DepOp::Update),
            Some("pip"),
            &explicit("uv"),
            None
        )
        .is_ok(),
        "compat on native update is ignored"
    );
    // Per-language tool sets: native Add/Remove/Update ignore any
    // opt-in (no delegated lib verbs left).
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Add),
            Some("cargo"),
            &explicit("cargo"),
            None
        )
        .is_ok()
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Add),
            Some("cargo"),
            &explicit("uv"),
            None
        )
        .is_ok(),
        "compat on native add is ignored"
    );
    assert!(
        gate(
            &ctx("lib", Some(eco::RUST), DepOp::Remove),
            Some("cargo"),
            &explicit("uv"),
            None
        )
        .is_ok(),
        "compat on native remove is ignored"
    );
    // Go remove runs natively on the mgc-written go.mod.
    for mode in [native(), explicit("go")] {
        assert!(
            gate(
                &ctx("lib", Some(eco::GO), DepOp::Remove),
                Some("go"),
                &mode,
                None
            )
            .is_ok(),
            "lib[go] remove is native"
        );
    }
    // Java/DOTNET Update run natively (resolve-latest + rewrite); gradle
    // projects fail closed in prepare_add/write_manifest.
    for lang in [eco::JAVA, eco::DOTNET] {
        for op in [DepOp::Update] {
            for mode in [native(), explicit("mvn"), explicit("dotnet")] {
                assert!(
                    gate(&ctx("lib", Some(lang), op), None, &mode, None).is_ok(),
                    "lib[{lang}] {} is native",
                    op.as_str()
                );
            }
        }
    }
    for op in [DepOp::Update] {
        for mode in [native(), explicit("dotnet")] {
            assert!(
                gate(&ctx("lib", Some(eco::DOTNET), op), None, &mode, None).is_ok(),
                "lib[dotnet] {} is native",
                op.as_str()
            );
        }
    }
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
    // uv vs pip on the NATIVE install/add lanes: compat is ignored
    // (native always runs) — no mismatch error. The flag==process
    // contract lives on the still-delegated ai verbs (Remove).
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Install),
            Some("uv"),
            &explicit("pip"),
            None
        )
        .is_ok(),
        "compat on native ai install is ignored"
    );
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Add),
            Some("uv"),
            &explicit("pip"),
            None
        )
        .is_ok(),
        "compat on native ai add is ignored"
    );
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Remove),
            Some("uv"),
            &explicit("pip"),
            None
        )
        .is_err(),
        "uv vs pip still mismatches on delegated ai remove"
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
    assert!(
        gate(
            &ctx("game", None, DepOp::Install),
            None,
            &explicit("cargo"),
            None
        )
        .is_err()
    );
    for fw in ["esp32-rust", "platformio", "zephyr"] {
        assert!(
            gate(
                &ctx("iot", Some(fw), DepOp::Install),
                None,
                &explicit("pio"),
                None
            )
            .is_ok(),
            "iot[{fw}] must open for its toolchain set"
        );
    }
    assert!(
        gate(
            &ctx("iot", None, DepOp::Install),
            None,
            &explicit("pio"),
            None
        )
        .is_err()
    );
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
        ("ai", Some(eco::PYTHON), DepOp::Remove),
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
    // Native install/add ignore any opt-in; delegated ai Remove still
    // enforces exact ownership.
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Install),
            Some("uv"),
            &explicit("cargo"),
            None
        )
        .is_ok(),
        "compat on native ai install is ignored"
    );
    assert!(
        gate(
            &ctx("ai", Some(eco::PYTHON), DepOp::Add),
            Some("uv"),
            &explicit("cargo"),
            None
        )
        .is_ok(),
        "compat on native ai add is ignored"
    );
    let err = gate(
        &ctx("ai", Some(eco::PYTHON), DepOp::Remove),
        Some("uv"),
        &explicit("cargo"),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("does not own"), "{err}");
    let err = gate(
        &ctx("game", Some(eco::BEVY), DepOp::Remove),
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
            // Exact cells without runners (never Delegated promises).
            // (lib is fully native now — only Update/unsupported cells of
            // other cores left.)
            ("app", Some(eco::SWIFT), DepOp::Add),
            ("app", Some(eco::KOTLIN), DepOp::Remove),
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
    // Reviewer table: flutter install + update native; add/remove
    // delegated; swift/kotlin install+list delegated; objc install only; everything else
    // Unsupported — including under compat.
    assert!(
        gate(
            &ctx("app", Some(eco::FLUTTER), DepOp::Update),
            Some("flutter"),
            &explicit("flutter"),
            None
        )
        .is_ok()
    );
    for lang in [eco::SWIFT, eco::KOTLIN] {
        let tool = if lang == eco::SWIFT {
            "swift"
        } else {
            "gradle"
        };
        assert!(
            gate(
                &ctx("app", Some(lang), DepOp::Install),
                None,
                &explicit(tool),
                None
            )
            .is_ok(),
            "app[{lang}] install opens for its toolchain"
        );
        assert!(
            gate(
                &ctx("app", Some(lang), DepOp::List),
                None,
                &explicit(tool),
                None
            )
            .is_ok(),
            "app[{lang}] list opens for its toolchain"
        );
    }
    assert!(
        gate(
            &ctx("app", Some(eco::OBJC), DepOp::Install),
            Some("xcodebuild"),
            &explicit("xcodebuild"),
            None
        )
        .is_ok()
    );
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
    assert_eq!(app_languages["swift"]["update"]["owner"], "mgc-native");
    // Exact app verbs: swift/kotlin add unsupported, objc list
    // unsupported, objc install delegated.
    assert_eq!(app_languages["swift"]["add"]["owner"], "unsupported");
    assert_eq!(app_languages["swift"]["install"]["owner"], "delegated");
    assert_eq!(app_languages["kotlin"]["remove"]["owner"], "unsupported");
    assert_eq!(app_languages["kotlin"]["list"]["owner"], "delegated");
    assert_eq!(app_languages["objc"]["list"]["owner"], "unsupported");
    assert_eq!(app_languages["objc"]["install"]["owner"], "delegated");
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
fn zephyr_add_remove_unsupported_install_update_delegated() {
    // Zephyr add/remove have NO runner (west.yml is hand-managed; the
    // adapter answers an honest not-supported error) — Unsupported even
    // with compat. Install/Update delegate to west.
    // (Zephyr add/remove không có runner — Unsupported.)
    for op in [DepOp::Add, DepOp::Remove] {
        assert!(
            gate(
                &ctx("iot", Some("zephyr"), op),
                Some("west"),
                &explicit("west"),
                None
            )
            .is_err(),
            "zephyr {op:?} must stay Unsupported"
        );
    }
    for op in [DepOp::Install, DepOp::Update] {
        assert!(
            gate(
                &ctx("iot", Some("zephyr"), op),
                Some("west"),
                &explicit("west"),
                None
            )
            .is_ok(),
            "zephyr {op:?} must delegate to west"
        );
    }
}
