#![allow(clippy::unwrap_used)]
//! Global Gate 1 — negative enforcement for capability claims.
//!
//! Two invariants are enforced against the REAL adapters (Global Gate 1,
//! Tech Lead 2026-09-16):
//!
//! 1. **Claim ⇒ backed**: every capability listed in `CAPABILITIES` must
//!    answer its probe with `Ok` (ProjectDetector answers `can_handle`).
//!    A claim with no implementation is blocked here — this is the
//!    "capability giả" gate.
//! 2. **No claim ⇒ fail-closed**: every capability NOT listed must answer
//!    with `MgError::Unsupported` (probes), and `DependencyResolver::resolve`
//!    in particular must fail closed for cores that do not claim it.
//!
//! Hai bất biến được ép trên adapter THẬT (Global Gate 1):
//! 1. **Claim ⇒ có thật**: mọi capability liệt kê trong `CAPABILITIES` phải
//!    trả `Ok` ở probe (ProjectDetector dùng `can_handle`). Claim không có
//!    implementation bị chặn ở đây — đây là gate "capability giả".
//! 2. **Không claim ⇒ fail-closed**: mọi capability KHÔNG liệt kê phải trả
//!    `MgError::Unsupported` (probe), và riêng `DependencyResolver::resolve`
//!    phải fail-closed với core không claim nó.
//!
//! Fixtures are `mgc.toml` markers (one temp project per core) — no network,
//! no toolchain execution: every probe is a lightweight claim check.
//! Fixture là marker `mgc.toml` (mỗi core một project tạm) — không network,
//! không chạy toolchain: mọi probe chỉ là kiểm tra claim nhẹ.

use futures_util::future::FutureExt;
use mgc_types::adapter::PackageAdapter;
use mgc_types::capabilities::*;
use mgc_types::{Ecosystem, Manifest, MgError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

static TMP_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Temp project fixture — one directory per core marker, removed on drop.
/// Fixture project tạm — mỗi core một thư mục marker, tự xoá khi drop.
struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(tag: &str, files: &[(&str, &str)]) -> Self {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "mgc-capabilities-{}-{}-{}",
            std::process::id(),
            seq,
            tag
        ));
        std::fs::create_dir_all(&root).unwrap();
        for (name, content) in files {
            std::fs::write(root.join(name), content).unwrap();
        }
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One constructible adapter under test: its lockstep fixture, its CLI core
/// name, and the built adapter.
/// Một adapter dựng được trong test: fixture tương ứng, tên core CLI và
/// adapter đã dựng.
struct Case {
    core: &'static str,
    adapter: Arc<dyn PackageAdapter>,
    fixture: TempProject,
}

fn build_cases() -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();

    let web_fixture = TempProject::new("web", &[("package.json", "{\"name\":\"t\"}")]);
    cases.push(Case {
        core: "web",
        adapter: Arc::new(mgc_web_adapter::WebAdapter::new().unwrap()),
        fixture: web_fixture,
    });

    let lib_fixture = TempProject::new(
        "lib",
        &[(
            "mgc.toml",
            "[ecosystem]\nname = \"t\"\n[lib]\nlanguage = \"rust\"\n",
        )],
    );
    cases.push(Case {
        core: "lib",
        adapter: Arc::new(
            mgc_lib_adapter::adapter_for(lib_fixture.path(), None, None)
                .unwrap()
                .unwrap(),
        ),
        fixture: lib_fixture,
    });

    let ai_fixture = TempProject::new(
        "ai",
        &[("mgc.toml", "[ai]\nframework = \"python-agent\"\n")],
    );
    cases.push(Case {
        core: "ai",
        adapter: Arc::new(mgc_ai_adapter::adapter_for(ai_fixture.path()).unwrap()),
        fixture: ai_fixture,
    });

    let app_fixture = TempProject::new("app", &[("mgc.toml", "[app]\nlanguage = \"flutter\"\n")]);
    cases.push(Case {
        core: "app",
        adapter: Arc::new(mgc_app_adapter::adapter_for(app_fixture.path()).unwrap()),
        fixture: app_fixture,
    });

    let game_fixture = TempProject::new("game", &[("mgc.toml", "[game]\nengine = \"bevy\"\n")]);
    cases.push(Case {
        core: "game",
        adapter: Arc::new(mgc_game_adapter::adapter_for(game_fixture.path()).unwrap()),
        fixture: game_fixture,
    });

    let iot_fixture = TempProject::new(
        "iot",
        &[("mgc.toml", "[iot]\nframework = \"esp32-rust\"\n")],
    );
    cases.push(Case {
        core: "iot",
        adapter: Arc::new(mgc_iot_adapter::adapter_for(iot_fixture.path()).unwrap()),
        fixture: iot_fixture,
    });

    // Terraform cloud type (no embedded web engine) — the strictest branch.
    // Cloud type terraform (không dựng web engine) — nhánh chặt nhất.
    let cloud_fixture = TempProject::new("clo", &[("mgc.toml", "[cloud]\ntype = \"terraform\"\n")]);
    cases.push(Case {
        core: "clo",
        adapter: Arc::new(
            mgc_cloud_adapter::adapter_for(cloud_fixture.path())
                .unwrap()
                .unwrap(),
        ),
        fixture: cloud_fixture,
    });

    let cicd_fixture = TempProject::new(
        "cicd",
        &[("mgc.toml", "[cicd]\nprovider = \"github-actions\"\n")],
    );
    cases.push(Case {
        core: "cicd",
        adapter: Arc::new(mgc_cicd_adapter::adapter_for(cicd_fixture.path()).unwrap()),
        fixture: cicd_fixture,
    });

    // Hardware detects on any mgc.toml.
    // Hardware detect bằng bất kỳ mgc.toml nào.
    let hw_fixture = TempProject::new("hardware", &[("mgc.toml", "[hardware]\nname = \"t\"\n")]);
    cases.push(Case {
        core: "hardware",
        adapter: Arc::new(mgc_hardware_adapter::adapter_for(hw_fixture.path()).unwrap()),
        fixture: hw_fixture,
    });

    cases
}

/// Capability probe — the representative answer for one claim.
/// Probe capability — câu trả lời đại diện cho một claim.
fn probe(adapter: &dyn PackageAdapter, cap: Capability, root: &Path) -> Result<bool, MgError> {
    match cap {
        // ProjectDetector's real method IS the probe: the fixture was built
        // to be detected, so a false answer means the claim is hollow.
        // Method thật của ProjectDetector CHÍNH LÀ probe: fixture được dựng
        // để detect được, nên trả false nghĩa là claim rỗng.
        Capability::ProjectDetector => Ok(adapter.can_handle(root)),
        Capability::ScaffoldProvider => adapter.probe_scaffold().map(|_| true),
        Capability::DependencyResolver => adapter.probe_dependency_resolver().map(|_| true),
        Capability::LockfileProvider => adapter.probe_lockfile_provider().map(|_| true),
        Capability::ArtifactFetcher => adapter.probe_artifact_fetcher().map(|_| true),
        Capability::ContentStoreProvider => adapter.probe_content_store().map(|_| true),
        Capability::Materializer => adapter.probe_materializer().map(|_| true),
        Capability::LifecycleRunner => adapter.probe_lifecycle_runner().map(|_| true),
        Capability::AuditProvider => adapter.probe_audit_provider().map(|_| true),
        Capability::OptimizerProvider => adapter.probe_optimizer().map(|_| true),
        Capability::SimulatorProvider => adapter.probe_simulator().map(|_| true),
        Capability::DeviceProvider => adapter.probe_device().map(|_| true),
        Capability::DeployProvider => adapter.probe_deploy().map(|_| true),
        Capability::ModelRuntimeProvider => adapter.probe_model_runtime().map(|_| true),
    }
}

fn is_unsupported(err: &MgError) -> bool {
    matches!(err, MgError::Unsupported { .. })
}

/// (1) Every CLAIMED capability must be backed (probe answers Ok).
/// (1) Mọi capability ĐÃ CLAIM phải có thật (probe trả Ok).
#[test]
fn claimed_capabilities_are_backed() {
    for case in build_cases() {
        let caps = case.adapter.capabilities();
        assert!(
            !caps.is_empty(),
            "{}: an adapter must declare at least one capability",
            case.core
        );
        for cap in caps {
            let result = probe(&*case.adapter, *cap, case.fixture.path());
            match result {
                Ok(true) => {}
                Ok(false) => panic!(
                    "{}: claims '{}' but its detection answers false — hollow claim",
                    case.core, cap
                ),
                Err(err) => panic!(
                    "{}: claims '{}' but its probe fails: {err} — capability không có implementation phải chặn",
                    case.core, cap
                ),
            }
        }
    }
}

/// (2) Every capability NOT claimed must fail closed (probe Unsupported).
/// (2) Mọi capability KHÔNG claim phải fail-closed (probe Unsupported).
#[test]
fn unclaimed_capabilities_fail_closed() {
    for case in build_cases() {
        let caps = case.adapter.capabilities();
        for cap in Capability::ALL {
            if caps.contains(cap) {
                continue;
            }
            let result = probe(&*case.adapter, *cap, case.fixture.path());
            match result {
                Err(err) if is_unsupported(&err) => {}
                Err(other) => panic!(
                    "{}: unclaimed '{}' must answer MgError::Unsupported, got {other}",
                    case.core, cap
                ),
                Ok(_) => panic!(
                    "{}: unclaimed '{}' answered Ok — a silent no-op would fake capability",
                    case.core, cap
                ),
            }
        }
    }
}

/// (3) Cores that do NOT claim DependencyResolver must answer `resolve`
/// with `MgError::Unsupported` (the hard negative case of the gate).
/// (3) Core KHÔNG claim DependencyResolver phải trả `MgError::Unsupported`
/// khi `resolve` (case âm tính cứng của gate).
#[test]
fn unclaimed_dependency_resolver_resolves_fail_closed() {
    let mut checked = 0usize;
    for case in build_cases() {
        if case
            .adapter
            .capabilities()
            .contains(&Capability::DependencyResolver)
        {
            continue;
        }
        let manifest = Manifest::new(case.core, case.adapter.ecosystem());
        let outcome = case.adapter.resolve(&manifest).now_or_never();
        let err = match outcome {
            Some(Ok(graph)) => panic!(
                "{}: unclaimed DependencyResolver returned Ok({} packages) — must fail closed",
                case.core,
                graph.len()
            ),
            Some(Err(err)) => err,
            None => panic!(
                "{}: resolve did not complete on first poll — the fail-closed default must be immediate",
                case.core
            ),
        };
        assert!(
            is_unsupported(&err),
            "{}: unclaimed resolve must answer MgError::Unsupported, got {err}",
            case.core
        );
        checked += 1;
    }
    // Hardware/Cicd/IoT/Game are the mandated negative cases (plus ai/app/clo).
    // Hardware/Cicd/IoT/Game là các case âm tính bắt buộc (thêm ai/app/clo).
    assert!(
        checked >= 4,
        "expected at least 4 cores without DependencyResolver, checked {checked}"
    );
}

/// (4) The claim list itself must be well-formed: subset of the 14 names,
/// no duplicates — a typo'd or duplicated claim can never ride along.
/// (4) Danh sách claim phải hợp lệ: tập con của 14 tên, không trùng — claim
/// gõ sai hay lặp không bao giờ đi ké được.
#[test]
fn capability_claims_are_well_formed() {
    assert_eq!(
        Capability::ALL.len(),
        14,
        "the gate defines 14 capabilities"
    );
    for case in build_cases() {
        let caps = case.adapter.capabilities();
        for (i, cap) in caps.iter().enumerate() {
            assert!(
                Capability::ALL.contains(cap),
                "{}: unknown capability {cap}",
                case.core
            );
            assert!(
                !caps[i + 1..].contains(cap),
                "{}: duplicate capability {cap}",
                case.core
            );
        }
    }
}

/// (5) The claim list must be exactly what each core declares — this pins
/// the matrix contract the lifecycle script reads from the binary.
/// (5) Danh sách claim phải đúng như từng core tuyên bố — ghim hợp đồng
/// matrix mà script lifecycle đọc từ binary.
#[test]
fn capability_map_matches_audited_contract() {
    let expected: &[(&str, &[Capability])] = &[
        (
            "web",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::DependencyResolver,
                Capability::LockfileProvider,
                Capability::ArtifactFetcher,
                Capability::ContentStoreProvider,
                Capability::Materializer,
                Capability::LifecycleRunner,
                Capability::AuditProvider,
            ],
        ),
        (
            "lib",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::DependencyResolver,
                Capability::LockfileProvider,
                Capability::ArtifactFetcher,
                Capability::ContentStoreProvider,
                Capability::AuditProvider,
            ],
        ),
        (
            "ai",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::LifecycleRunner,
                Capability::OptimizerProvider,
                Capability::AuditProvider,
            ],
        ),
        (
            "app",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::LifecycleRunner,
                Capability::ContentStoreProvider,
                Capability::LockfileProvider,
                Capability::AuditProvider,
                // Phase 2: app resolves Dart deps through the native
                // pub.dev engine (PubProtocol) — DependencyResolver is now
                // real for flutter projects.
                // (Phase 2: app resolve Dart deps qua engine pub.dev native
                // (PubProtocol) — DependencyResolver giờ là thật cho
                // project flutter.)
                Capability::DependencyResolver,
            ],
        ),
        (
            "game",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::ContentStoreProvider,
                Capability::AuditProvider,
            ],
        ),
        (
            "iot",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::ContentStoreProvider,
                Capability::AuditProvider,
            ],
        ),
        (
            "clo",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::DeployProvider,
                Capability::LifecycleRunner,
                Capability::ContentStoreProvider,
                Capability::AuditProvider,
            ],
        ),
        (
            "cicd",
            &[
                Capability::ProjectDetector,
                Capability::ScaffoldProvider,
                Capability::AuditProvider,
            ],
        ),
        (
            "hardware",
            &[
                Capability::ProjectDetector,
                Capability::OptimizerProvider,
                Capability::AuditProvider,
            ],
        ),
    ];

    let cases = build_cases();
    assert_eq!(cases.len(), expected.len(), "one case per core");
    for case in &cases {
        let want = expected
            .iter()
            .find(|(core, _)| *core == case.core)
            .map(|(_, caps)| *caps)
            .unwrap_or(&[]);
        assert_eq!(
            case.adapter.capabilities(),
            want,
            "{}: capability map drifted from the audited contract",
            case.core
        );
    }
}

/// (6) `core_id` must be the CLI-canonical name the matrix lanes use — a
/// drift here silently mislabels every fail-closed error.
/// (6) `core_id` phải là tên chuẩn CLI mà lane matrix dùng — lệch ở đây làm
/// nhãn mọi lỗi fail-closed sai.
#[test]
fn core_ids_are_cli_canonical() {
    for case in build_cases() {
        assert_eq!(
            case.adapter.core_id(),
            case.core,
            "{}: core_id must match the CLI/matrix core name",
            case.core
        );
        assert!(!case.adapter.name().is_empty(), "{}: name", case.core);
        assert_eq!(
            case.adapter.ecosystem(),
            match case.core {
                "web" => Ecosystem::Web,
                "lib" => Ecosystem::Lib,
                "ai" => Ecosystem::Ai,
                "app" => Ecosystem::App,
                "game" => Ecosystem::Game,
                "iot" => Ecosystem::Iot,
                "clo" => Ecosystem::Cloud,
                "cicd" => Ecosystem::Cicd,
                "hardware" => Ecosystem::Hardware,
                other => panic!("unknown core {other}"),
            },
            "{}: ecosystem",
            case.core
        );
    }
}
