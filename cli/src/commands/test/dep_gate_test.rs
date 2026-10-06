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
    let context = DepContext::new(core, ecosystem, None, None, op);
    if core != "lib" {
        return context;
    }
    let format = match ecosystem {
        Some(eco::TS) => Some("package-json"),
        Some(eco::RUST) => Some("cargo-toml"),
        Some(eco::PYTHON) => Some("pep621-native"),
        Some(eco::GO) => Some("go-mod"),
        Some(eco::JAVA) => Some("maven-pom"),
        Some(eco::DOTNET) => Some("csproj"),
        _ => None,
    };
    context.with_manifest_format(format)
}

fn lib_ctx(
    ecosystem: &'static str,
    manifest_format: Option<&'static str>,
    op: DepOp,
) -> DepContext<'static> {
    DepContext::new("lib", Some(ecosystem), None, None, op).with_manifest_format(manifest_format)
}

#[test]
fn java_and_dotnet_ownership_requires_an_mgc_owned_manifest_format() {
    for op in [DepOp::Install, DepOp::Add, DepOp::Remove, DepOp::Update] {
        assert!(matches!(
            owner_for(&lib_ctx(eco::JAVA, None, op)),
            DepOwner::Unsupported
        ));
        assert!(matches!(
            owner_for(&lib_ctx(eco::JAVA, Some("gradle"), op)),
            DepOwner::Unsupported
        ));
        assert!(matches!(
            owner_for(&lib_ctx(eco::JAVA, Some("maven-pom"), op)),
            DepOwner::Native
        ));
        assert!(matches!(
            owner_for(&lib_ctx(eco::DOTNET, None, op)),
            DepOwner::Unsupported
        ));
        assert!(matches!(
            owner_for(&lib_ctx(eco::DOTNET, Some("csproj"), op)),
            DepOwner::Native
        ));
    }
}

#[test]
fn every_lib_native_lane_requires_its_concrete_owned_manifest_format() {
    let cases = [
        (eco::TS, "package-json"),
        (eco::RUST, "cargo-toml"),
        (eco::PYTHON, "pep621-native"),
        (eco::GO, "go-mod"),
        (eco::JAVA, "maven-pom"),
        (eco::DOTNET, "csproj"),
    ];
    for (ecosystem, format) in cases {
        for op in [DepOp::Install, DepOp::Add, DepOp::Remove, DepOp::Update] {
            assert!(
                matches!(
                    owner_for(&lib_ctx(ecosystem, None, op)),
                    DepOwner::Unsupported
                ),
                "lib/{ecosystem}/{op:?} without a manifest must be unsupported"
            );
            assert!(
                matches!(
                    owner_for(&lib_ctx(ecosystem, Some(format), op)),
                    DepOwner::Native
                ),
                "lib/{ecosystem}/{format}/{op:?} should remain native"
            );
        }
    }
    for unsupported in ["pyproject-unsupported", "requirements-txt", "foreign-lock"] {
        assert!(matches!(
            owner_for(&lib_ctx(eco::PYTHON, Some(unsupported), DepOp::Install)),
            DepOwner::Unsupported
        ));
    }
}

#[test]
fn ai_python_ownership_requires_the_mgc_owned_pyproject_lane() {
    for op in DepOp::ALL {
        assert!(
            matches!(
                owner_for(&ctx("ai", Some(eco::PYTHON), *op)),
                DepOwner::Unsupported
            ),
            "ai/python/{:?} must not be advertised without an owned manifest",
            op
        );
        let owned = DepContext::new("ai", Some(eco::PYTHON), None, None, *op)
            .with_manifest_format(Some("mgc-pyproject"));
        let expected_native = !matches!(op, DepOp::OfflineReinstall | DepOp::Gc);
        assert_eq!(
            matches!(owner_for(&owned), DepOwner::Native),
            expected_native,
            "ai/python/{op:?} ownership must match implemented operations"
        );
    }
}

#[cfg(any(feature = "lib", feature = "ai"))]
#[test]
fn lib_runtime_context_uses_the_project_manifest_format() {
    let pom = tempfile::tempdir().expect("Maven fixture");
    std::fs::write(pom.path().join("pom.xml"), "<project/>").expect("write POM");
    assert!(matches!(
        owner_for(&crate::commands::dep_gate::lib_project_context(
            pom.path(),
            DepOp::Install
        )),
        DepOwner::Native
    ));

    let gradle = tempfile::tempdir().expect("Gradle fixture");
    std::fs::write(gradle.path().join("build.gradle"), "plugins {}").expect("write Gradle script");
    let gradle_context =
        crate::commands::dep_gate::lib_project_context(gradle.path(), DepOp::Install);
    assert!(matches!(owner_for(&gradle_context), DepOwner::Unsupported));
    assert_eq!(gradle_context.describe(), "lib[java|manifest:gradle]");
    let error = gate(&gradle_context, None, &native(), None)
        .expect_err("Gradle must be rejected before entering the adapter")
        .to_string();
    assert!(error.contains("Gradle build scripts are not dependency manifests"));

    let mixed = tempfile::tempdir().expect("mixed Maven/Gradle fixture");
    std::fs::write(mixed.path().join("pom.xml"), "<project/>").expect("write POM");
    std::fs::write(mixed.path().join("build.gradle"), "plugins {}").expect("write Gradle script");
    let mixed_context =
        crate::commands::dep_gate::lib_project_context(mixed.path(), DepOp::Install);
    assert!(matches!(owner_for(&mixed_context), DepOwner::Unsupported));
    let error = gate(&mixed_context, None, &native(), None)
        .expect_err("mixed build systems must not be silently assigned to Maven")
        .to_string();
    assert!(error.contains("both Maven and Gradle manifests"));

    let empty_dotnet = tempfile::tempdir().expect("empty .NET fixture");
    assert!(matches!(
        owner_for(&crate::commands::dep_gate::lib_project_context(
            empty_dotnet.path(),
            DepOp::Add
        )),
        DepOwner::Unsupported
    ));
    std::fs::write(empty_dotnet.path().join("Project.CSPROJ"), "<Project/>").expect("write csproj");
    assert!(matches!(
        owner_for(&crate::commands::dep_gate::lib_project_context(
            empty_dotnet.path(),
            DepOp::Add
        )),
        DepOwner::Native
    ));
    std::fs::write(empty_dotnet.path().join("Second.csproj"), "<Project/>")
        .expect("write second csproj");
    assert!(matches!(
        owner_for(&crate::commands::dep_gate::lib_project_context(
            empty_dotnet.path(),
            DepOp::Add
        )),
        DepOwner::Unsupported
    ));
}

#[cfg(feature = "lib")]
#[test]
fn lib_runtime_context_rejects_missing_and_foreign_manifest_shapes() {
    use crate::commands::dep_gate::lib_project_context;

    let cases = [
        (eco::TS, "package.json", "{}\n"),
        (
            eco::RUST,
            "Cargo.toml",
            "[package]\nname='x'\nversion='0.1.0'\n",
        ),
        (eco::GO, "go.mod", "module example.test/x\n\ngo 1.24\n"),
        (
            eco::PYTHON,
            "pyproject.toml",
            "[project]\nname='x'\nversion='0.1.0'\ndependencies=[]\n",
        ),
    ];
    for (ecosystem, manifest, contents) in cases {
        let missing = tempfile::tempdir().unwrap();
        std::fs::write(
            missing.path().join("mgc.toml"),
            format!("ecosystem='lib'\n\n[lib]\nlanguage='{ecosystem}'\n"),
        )
        .unwrap();
        assert!(
            matches!(
                owner_for(&lib_project_context(missing.path(), DepOp::Install)),
                DepOwner::Unsupported
            ),
            "missing {manifest} must not be advertised native"
        );

        let owned = tempfile::tempdir().unwrap();
        std::fs::write(
            owned.path().join("mgc.toml"),
            format!("ecosystem='lib'\n\n[lib]\nlanguage='{ecosystem}'\n"),
        )
        .unwrap();
        std::fs::write(owned.path().join(manifest), contents).unwrap();
        assert!(
            matches!(
                owner_for(&lib_project_context(owned.path(), DepOp::Install)),
                DepOwner::Native
            ),
            "owned {manifest} should remain native"
        );
    }

    let foreign_python = tempfile::tempdir().unwrap();
    std::fs::write(
        foreign_python.path().join("mgc.toml"),
        "ecosystem='lib'\n\n[lib]\nlanguage='python'\n",
    )
    .unwrap();
    std::fs::write(
        foreign_python.path().join("pyproject.toml"),
        "[project]\nname='x'\nversion='0.1.0'\ndependencies=[]\n",
    )
    .unwrap();
    std::fs::write(foreign_python.path().join("uv.lock"), "version = 1\n").unwrap();
    assert!(matches!(
        owner_for(&lib_project_context(foreign_python.path(), DepOp::Install)),
        DepOwner::Unsupported
    ));
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
fn store_gc_is_not_inherited_from_an_install_owner() {
    for (core, ecosystem) in [
        ("ai", Some(eco::PYTHON)),
        ("app", Some(eco::FLUTTER)),
        ("lib", Some(eco::RUST)),
        ("game", Some(eco::BEVY)),
        ("iot", Some("esp32-rust")),
    ] {
        assert!(
            matches!(
                owner_for(&ctx(core, ecosystem, DepOp::Gc)),
                DepOwner::Unsupported
            ),
            "{core}/{ecosystem:?}/gc must not inherit install ownership"
        );
        assert!(gate(&ctx(core, ecosystem, DepOp::Gc), None, &native(), None).is_err());
    }

    for ecosystem in [Some(eco::JS), Some(eco::TS)] {
        assert!(matches!(
            owner_for(&ctx("web", ecosystem, DepOp::Gc)),
            DepOwner::Native
        ));
        assert!(gate(&ctx("web", ecosystem, DepOp::Gc), None, &native(), None).is_ok());
    }
}

#[test]
fn offline_reinstall_is_not_inherited_from_online_install() {
    for (core, ecosystem) in [
        ("app", Some(eco::FLUTTER)),
        ("app", Some(eco::SWIFT)),
        ("lib", Some(eco::RUST)),
        ("lib", Some(eco::GO)),
        ("lib", Some(eco::JAVA)),
        ("lib", Some(eco::DOTNET)),
        ("game", Some(eco::BEVY)),
        ("iot", Some("esp32-rust")),
    ] {
        assert!(
            matches!(
                owner_for(&ctx(core, ecosystem, DepOp::OfflineReinstall)),
                DepOwner::Unsupported
            ),
            "{core}/{ecosystem:?} must not inherit online install as offline support"
        );
        assert!(
            gate(
                &ctx(core, ecosystem, DepOp::OfflineReinstall),
                None,
                &native(),
                None
            )
            .is_err()
        );
    }

    let lib_python_native = DepContext::new(
        "lib",
        Some(eco::PYTHON),
        None,
        None,
        DepOp::OfflineReinstall,
    )
    .with_manifest_format(Some("pep621-native"));
    assert!(matches!(owner_for(&lib_python_native), DepOwner::Native));
    assert!(gate(&lib_python_native, None, &native(), None).is_ok());

    let lib_python_foreign = DepContext::new(
        "lib",
        Some(eco::PYTHON),
        None,
        None,
        DepOp::OfflineReinstall,
    )
    .with_manifest_format(Some("requirements-txt"));
    assert!(matches!(
        owner_for(&lib_python_foreign),
        DepOwner::Unsupported
    ));

    let ai_python_native =
        DepContext::new("ai", Some(eco::PYTHON), None, None, DepOp::OfflineReinstall)
            .with_manifest_format(Some("mgc-pyproject"));
    assert!(matches!(
        owner_for(&ai_python_native),
        DepOwner::Unsupported
    ));
    assert!(gate(&ai_python_native, None, &native(), None).is_err());

    for (core, ecosystem) in [
        ("web", Some(eco::JS)),
        ("web", Some(eco::TS)),
        ("lib", Some(eco::TS)),
    ] {
        assert!(matches!(
            owner_for(&ctx(core, ecosystem, DepOp::OfflineReinstall)),
            DepOwner::Native
        ));
    }
    let cloud_cdk = DepContext::new(
        "clo",
        Some(eco::JS),
        Some("cdk"),
        None,
        DepOp::OfflineReinstall,
    );
    assert!(matches!(owner_for(&cloud_cdk), DepOwner::Unsupported));
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
    for lang in [eco::RUST, eco::PYTHON, eco::GO] {
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
    for (lang, format) in [(eco::JAVA, "maven-pom"), (eco::DOTNET, "csproj")] {
        for op in [DepOp::Install, DepOp::Add, DepOp::Remove, DepOp::Update] {
            assert!(matches!(
                owner_for(&lib_ctx(lang, Some(format), op)),
                DepOwner::Native
            ));
        }
    }
    // Native Add (resolve-first + mgc-side manifest edit, zero spawn):
    // Rust/Python/Go are language-owned; Java/.NET require explicit POM /
    // single-csproj formats. Gradle is rejected by the gate before adapter.
    for lang in [eco::RUST, eco::PYTHON, eco::GO] {
        assert!(
            gate(&ctx("lib", Some(lang), DepOp::Add), None, &native(), None).is_ok(),
            "lib[{lang}] add is native (no toolchain spawn)"
        );
    }
    for (lang, format) in [(eco::JAVA, "maven-pom"), (eco::DOTNET, "csproj")] {
        assert!(
            gate(
                &lib_ctx(lang, Some(format), DepOp::Add),
                None,
                &native(),
                None
            )
            .is_ok(),
            "lib[{lang}/{format}] add is native (no toolchain spawn)"
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
    // Java POM/.NET csproj Update run natively; Gradle and unknown formats
    // fail at the manifest-aware gate, before adapter construction.
    for (lang, format) in [(eco::JAVA, "maven-pom"), (eco::DOTNET, "csproj")] {
        assert!(
            gate(
                &lib_ctx(lang, Some(format), DepOp::Update),
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
        let context = DepContext::new("clo", Some(eco::JS), Some(framework), None, DepOp::List)
            .with_manifest_format(Some("package-json"));
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
            let context = DepContext::new("clo", Some(eco::JS), Some(framework), None, op)
                .with_manifest_format(Some("package-json"));
            assert!(
                matches!(owner_for(&context), DepOwner::Native),
                "clo/{framework} {} must use the embedded MGC JS dependency engine",
                op.as_str()
            );
        }
        let missing_manifest =
            DepContext::new("clo", Some(eco::JS), Some(framework), None, DepOp::Install);
        assert!(matches!(
            owner_for(&missing_manifest),
            DepOwner::Unsupported
        ));
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
    assert!(
        gate_cloud_project(
            missing_manifest.path(),
            "cdk",
            DepOp::OfflineReinstall,
            None
        )
        .is_err()
    );

    let js_project = tempfile::tempdir().unwrap();
    std::fs::write(js_project.path().join("package.json"), "{}\n").unwrap();
    assert!(gate_cloud_project(js_project.path(), "cdk", DepOp::Install, None).is_ok());
    assert!(gate_cloud_project(js_project.path(), "pulumi", DepOp::Add, None).is_ok());
    assert!(gate_cloud_project(js_project.path(), "cdk", DepOp::OfflineReinstall, None).is_ok());
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
        assert!(
            gate(&ctx("ai", Some(eco::PYTHON), op), None, &native(), None).is_err(),
            "AI Python {op:?} without an owned manifest must fail closed"
        );
        let owned = DepContext::new("ai", Some(eco::PYTHON), None, None, op)
            .with_manifest_format(Some("mgc-pyproject"));
        assert!(gate(&owned, None, &native(), None).is_ok());
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
    for (core, ecosystem, framework) in [
        ("game", eco::BEVY, eco::BEVY),
        ("iot", "esp32-rust", "esp32-rust"),
    ] {
        for op in [DepOp::Install, DepOp::Add, DepOp::Update, DepOp::Remove] {
            let without_manifest =
                DepContext::new(core, Some(ecosystem), Some(framework), None, op);
            assert!(
                matches!(owner_for(&without_manifest), DepOwner::Unsupported),
                "{core}/{framework}/{op:?} without Cargo.toml must be unsupported"
            );
            let with_manifest = without_manifest.with_manifest_format(Some("cargo-toml"));
            assert!(
                matches!(owner_for(&with_manifest), DepOwner::Native),
                "{core}/{framework}/{op:?} with Cargo.toml is native"
            );
        }
    }
    let bevy =
        ctx("game", Some(eco::BEVY), DepOp::Install).with_manifest_format(Some("cargo-toml"));
    let esp32 =
        ctx("iot", Some("esp32-rust"), DepOp::Install).with_manifest_format(Some("cargo-toml"));
    assert!(gate(&bevy, None, &native(), None).is_ok());
    assert!(gate(&ctx("game", None, DepOp::Install), None, &native(), None).is_err());
    assert!(matches!(owner_for(&esp32), DepOwner::Native));
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
    // Splitting cores expose per-ecosystem overrides with their concrete
    // manifest formats; the unspecialized base remains unsupported.
    let lib_languages = dependency_ownership("lib")["languages"].clone();
    assert_eq!(lib_languages["ts"]["add"]["owner"], "mgc-native");
    assert_eq!(lib_languages["rust"]["install"]["owner"], "mgc-native");
    assert_eq!(lib_languages["rust"]["add"]["owner"], "mgc-native");
    // Native capability rows carry no external dependency-manager tools.
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
    let game_languages = dependency_ownership("game")["languages"].clone();
    assert_eq!(game_languages["bevy"]["install"]["owner"], "mgc-native");
    assert_eq!(game_languages["bevy"]["install"]["requires"], "Cargo.toml");
    let iot_languages = dependency_ownership("iot")["languages"].clone();
    assert_eq!(iot_languages["esp32-rust"]["add"]["owner"], "mgc-native");
    assert_eq!(iot_languages["esp32-rust"]["add"]["requires"], "Cargo.toml");
    let lib_operations = &dependency_ownership("lib")["operations"];
    assert_eq!(lib_operations["add"]["owner"], "unsupported");
    assert_eq!(lib_operations["install"]["owner"], "unsupported");
    let variants = &dependency_ownership("lib")["manifest_variants"];
    assert_eq!(
        variants["ts/package-json"]["install"]["owner"],
        "mgc-native"
    );
    assert_eq!(variants["ts/unknown"]["install"]["owner"], "unsupported");
    assert_eq!(variants["rust/cargo-toml"]["add"]["owner"], "mgc-native");
    assert_eq!(variants["rust/unknown"]["add"]["owner"], "unsupported");
    assert_eq!(
        variants["python/pep621-native"]["install"]["owner"],
        "mgc-native"
    );
    assert_eq!(
        variants["python/pyproject-unsupported"]["install"]["owner"],
        "unsupported"
    );
    assert_eq!(variants["go/go-mod"]["install"]["owner"], "mgc-native");
    assert_eq!(variants["go/unknown"]["install"]["owner"], "unsupported");
    assert_eq!(variants["java/maven-pom"]["add"]["owner"], "mgc-native");
    assert_eq!(variants["java/maven-pom"]["install"]["owner"], "mgc-native");
    assert_eq!(variants["java/gradle"]["add"]["owner"], "unsupported");
    assert_eq!(variants["java/gradle"]["install"]["owner"], "unsupported");
    assert_eq!(variants["dotnet/csproj"]["update"]["owner"], "mgc-native");
    assert_eq!(variants["java/unknown"]["add"]["owner"], "unsupported");
    assert_eq!(
        variants["dotnet/unknown"]["install"]["owner"],
        "unsupported"
    );
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

#[cfg(not(any(feature = "lib", feature = "ai")))]
#[test]
fn lib_project_context_fails_closed_when_lib_adapter_is_not_compiled() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='sample'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();

    let context = lib_project_context(root.path(), DepOp::Install);

    assert!(context.ecosystem.is_none());
    assert!(matches!(owner_for(&context), DepOwner::Unsupported));
}
