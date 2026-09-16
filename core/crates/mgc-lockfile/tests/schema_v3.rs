//! Tests for mgc.lock schema v3 — canonical unified dependency graph (Phase 1).
//! Tests schema v3 của mgc.lock — đồ thị phụ thuộc hợp nhất canonical (Phase 1).
//!
//! Covers: v3 roundtrip, v2 backward-compatible parse (TOML + JSON payload),
//! v2→v3 migration, export skeletons, ownership-ledger loading, and the
//! ownership-completeness gate.
//! Bao gồm: roundtrip v3, parse tương thích v2 (payload TOML + JSON),
//! migration v2→v3, export skeleton, nạp ledger sở hữu, và cổng đầy đủ
//! quyền sở hữu.

use mgc_lockfile::ecosystem_tag::EcosystemTag;
use mgc_lockfile::export::{ExportTarget, export_canonical};
use mgc_lockfile::migrate::{detect_lockfile_version, migrate_v2_to_v3};
use mgc_lockfile::schema::{
    ArtifactRef, CrossEdge, Lockfile, OwnershipEntry, Package, Provenance,
    SOURCE_KIND_DELEGATED_TOOL, SOURCE_KIND_REGISTRY_IMPORT, WorkspaceTopology,
};
use mgc_lockfile::serialization::to_toml;
use mgc_lockfile::{
    LOCKFILE_SCHEMA_VERSION, LockfileError, load_ownership_ledger, parse_lockfile,
    verify_ownership_completeness,
};

/// A minimal v2-shaped TOML lockfile (no v3 fields present).
/// Lockfile TOML shape v2 tối thiểu (không có field v3 nào).
const V2_TOML: &str = r#"
version = "2"
[metadata]
generated_at = "2026-08-21T18:30:00+07:00"
generator = "mgc/1.0.0"
lockfile_hash = ""
[[package]]
name = "react"
version = "18.2.0"
resolved = "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
integrity = "blake3-abc123"
dependencies = []
"#;

#[test]
fn v3_roundtrip_preserves_all_new_fields() {
    let mut lock = Lockfile::new();
    assert_eq!(lock.version, LOCKFILE_SCHEMA_VERSION);

    lock.optimizer_profile = Some("size".to_string());
    lock.workspace = Some(WorkspaceTopology {
        members: vec!["web".to_string(), "ai".to_string()],
        cross_core_edges: vec![CrossEdge {
            from_core: "web".to_string(),
            to_core: "ai".to_string(),
            package: "shared-model".to_string(),
        }],
    });
    lock.metadata.dependency_ownership = vec![OwnershipEntry {
        core: "rust".to_string(),
        owner: "delegated".to_string(),
        tool: Some("cargo 1.85".to_string()),
        documented_at: "2026-09-16T00:00:00Z".to_string(),
    }];

    let mut pkg = Package::new(
        "serde".to_string(),
        "1.0.219".to_string(),
        "https://static.crates.io/crates/serde/serde-1.0.219.crate".to_string(),
        "blake3-abc".to_string(),
    );
    pkg.ecosystem = EcosystemTag::Rust;
    pkg.registry = Some("crates://sparse.crates.io".to_string());
    pkg.artifact = Some(ArtifactRef {
        url: "https://static.crates.io/crates/serde/serde-1.0.219.crate".to_string(),
        size_bytes: Some(1024),
        content_hash: "abcd1234".to_string(),
        downloaded_from: "crates://sparse.crates.io".to_string(),
    });
    pkg.provenance = Some(Provenance {
        source_kind: SOURCE_KIND_REGISTRY_IMPORT.to_string(),
        tool: Some("cargo 1.85".to_string()),
        imported_from: Some("Cargo.lock".to_string()),
    });
    pkg.markers = Some(vec!["sys-win32".to_string()]);
    pkg.extras = Some(vec!["derive".to_string()]);
    pkg.peers = Some(vec!["serde_core".to_string()]);
    pkg.store_ref = Some("cas/files/blake3/ab/abcd1234".to_string());
    pkg.toolchain = Some("rust 1.85".to_string());
    pkg.scripts_policy = Some("deny".to_string());
    pkg.add_dependency("serde_core@1.0.0".to_string());
    lock.add_package(pkg);

    // Serialize (v3 writer) → parse back (v2/v3 gate) → every new field intact.
    // Serialize (writer v3) → parse ngược (cổng v2/v3) → mọi field mới nguyên vẹn.
    let encoded = to_toml(&lock).unwrap();
    let decoded = parse_lockfile(&encoded).unwrap();
    assert_eq!(decoded, lock);
}

#[test]
fn v2_lockfile_parses_with_v3_defaults() {
    // v2 files stay readable: version "2" passes the gate and every v3
    // field falls back to its serde default.
    // File v2 vẫn đọc được: version "2" qua cổng gate và mọi field v3 về
    // mặc định serde.
    let lock = parse_lockfile(V2_TOML).unwrap();
    assert_eq!(lock.version, "2");

    let pkg = lock.get_package("react").unwrap();
    assert_eq!(pkg.ecosystem, EcosystemTag::Other);
    assert_eq!(pkg.registry, None);
    assert_eq!(pkg.artifact, None);
    assert_eq!(pkg.provenance, None);
    assert_eq!(pkg.markers, None);
    assert_eq!(pkg.extras, None);
    assert_eq!(pkg.peers, None);
    assert_eq!(pkg.store_ref, None);
    assert_eq!(pkg.toolchain, None);
    assert_eq!(pkg.scripts_policy, None);
    assert_eq!(lock.workspace, None);
    assert_eq!(lock.optimizer_profile, None);
    assert!(lock.metadata.dependency_ownership.is_empty());
}

#[test]
fn v2_json_payload_parses_with_v3_defaults() {
    // The design sketch names a JSON v2 payload — the serde defaults must
    // hold regardless of the serialization carrying them.
    // Bản thiết kế nêu payload JSON v2 — mặc định serde phải giữ đúng bất
    // kể serialization nào mang nó.
    let json_v2 = r#"{
        "version": "2",
        "metadata": {
            "generated_at": "2026-08-21T18:30:00+07:00",
            "generator": "mgc/1.0.0",
            "lockfile_hash": ""
        },
        "package": [
            {
                "name": "react",
                "version": "18.2.0",
                "resolved": "https://registry.npmjs.org/react/-/react-18.2.0.tgz",
                "integrity": "blake3-abc123",
                "dependencies": []
            }
        ]
    }"#;

    let lock: Lockfile = serde_json::from_str(json_v2).unwrap();
    assert_eq!(lock.version, "2");
    assert_eq!(lock.packages.len(), 1);
    assert_eq!(lock.packages[0].ecosystem, EcosystemTag::Other);
    assert_eq!(lock.packages[0].provenance, None);
    assert_eq!(lock.packages[0].registry, None);
}

#[test]
fn migration_v2_to_v3_fills_other_ecosystem_and_registry_import_provenance() {
    assert_eq!(detect_lockfile_version(V2_TOML).unwrap(), 2);

    let lock = parse_lockfile(V2_TOML).unwrap();
    let migrated = migrate_v2_to_v3(lock);

    assert_eq!(migrated.version, LOCKFILE_SCHEMA_VERSION);
    assert_eq!(migrated.packages.len(), 1);
    let pkg = &migrated.packages[0];
    // v2 carries no ecosystem data → stays `other`.
    // v2 không mang dữ liệu ecosystem → giữ `other`.
    assert_eq!(pkg.ecosystem, EcosystemTag::Other);
    // Empty provenance is filled with the registry-import kind.
    // Provenance trống được điền kind registry-import.
    let provenance = pkg.provenance.as_ref().unwrap();
    assert_eq!(provenance.source_kind, SOURCE_KIND_REGISTRY_IMPORT);
    assert_eq!(provenance.tool, None);
    assert_eq!(provenance.imported_from, None);

    // Migration must not clobber existing ecosystem data when run on a v3 lock.
    // Migration chạy trên lock v3 không được xoá dữ liệu ecosystem sẵn có.
    let mut v3 = Lockfile::new();
    let mut pkg = Package::new(
        "syn".to_string(),
        "2.0.0".to_string(),
        "u".to_string(),
        "b".to_string(),
    );
    pkg.ecosystem = EcosystemTag::Rust;
    v3.add_package(pkg);
    let migrated = migrate_v2_to_v3(v3);
    assert_eq!(migrated.packages[0].ecosystem, EcosystemTag::Rust);
}

#[test]
fn export_skeletons_route_by_ecosystem() {
    let mut lock = Lockfile::new();

    let mut serde_pkg = Package::new(
        "serde".to_string(),
        "1.0.219".to_string(),
        "https://static.crates.io/crates/serde/serde-1.0.219.crate".to_string(),
        "blake3-r1".to_string(),
    );
    serde_pkg.ecosystem = EcosystemTag::Rust;
    serde_pkg.registry = Some("crates://sparse.crates.io".to_string());

    let mut numpy_pkg = Package::new(
        "numpy".to_string(),
        "1.26.4".to_string(),
        "https://files.pythonhosted.org/numpy".to_string(),
        "blake3-p1".to_string(),
    );
    numpy_pkg.ecosystem = EcosystemTag::Python;

    let mut react_pkg = Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-w1".to_string(),
    );
    react_pkg.ecosystem = EcosystemTag::Web;

    lock.add_package(serde_pkg);
    lock.add_package(numpy_pkg);
    lock.add_package(react_pkg);

    // Cargo.lock: only the rust pin, mapped to Cargo's registry source shape.
    // Cargo.lock: chỉ pin rust, ánh xạ sang shape registry source của Cargo.
    let cargo =
        String::from_utf8(export_canonical(&lock, ExportTarget::CargoLock).unwrap()).unwrap();
    assert!(cargo.contains("[[package]]"));
    assert!(cargo.contains("name = \"serde\""));
    assert!(cargo.contains("version = \"1.0.219\""));
    assert!(cargo.contains("source = \"registry+https://sparse.crates.io\""));
    assert!(!cargo.contains("react"));
    assert!(!cargo.contains("numpy"));

    // requirements.txt: `name==version` for python only.
    // requirements.txt: `name==version` chỉ cho python.
    let reqs =
        String::from_utf8(export_canonical(&lock, ExportTarget::RequirementsTxt).unwrap()).unwrap();
    assert!(reqs.contains("numpy==1.26.4"));
    assert!(!reqs.contains("serde=="));
    assert!(!reqs.contains("react=="));

    // package-lock.json: parseable JSON, lockfileVersion 3, web pin present.
    // package-lock.json: JSON parse được, lockfileVersion 3, có pin web.
    let pkg_lock = export_canonical(&lock, ExportTarget::PackageLock).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&pkg_lock).unwrap();
    assert_eq!(doc["lockfileVersion"].as_i64(), Some(3));
    assert_eq!(
        doc["packages"]["node_modules/react"]["version"].as_str(),
        Some("18.2.0")
    );
    assert!(doc["packages"].get("node_modules/serde").is_none());

    // go.sum: comment-only skeleton — nothing parseable leaks out.
    // go.sum: skeleton chỉ toàn comment — không lọt dòng parse được ra ngoài.
    let go_sum = String::from_utf8(export_canonical(&lock, ExportTarget::GoSum).unwrap()).unwrap();
    assert!(go_sum.contains("skeleton — go toolchain regenerates"));
    for line in go_sum.lines() {
        assert!(
            line.is_empty() || line.starts_with('#'),
            "non-comment line: {line}"
        );
    }
}

#[test]
fn export_fails_closed_on_empty_version() {
    let mut lock = Lockfile::new();
    let mut pkg = Package::new(
        "broken".to_string(),
        String::new(),
        "https://example".to_string(),
        "blake3-b".to_string(),
    );
    pkg.ecosystem = EcosystemTag::Rust;
    lock.add_package(pkg);

    // A matching-ecosystem pin without a version must fail loudly (no
    // silent skip — the version comes out as an empty pin otherwise).
    // Pin đúng ecosystem nhưng thiếu version phải fail rõ ràng (không skip
    // âm thầm — nếu không sẽ ra pin rỗng version).
    let err = export_canonical(&lock, ExportTarget::CargoLock).unwrap_err();
    assert!(err.to_string().contains("empty version"), "{err}");
}

#[test]
fn ownership_ledger_loads_deduped_entries() {
    // Realistic slice of docs/specs/dependencyDelegationAudit.json (the
    // findings shape the repo's audit scanner emits).
    // Đoạn thật của docs/specs/dependencyDelegationAudit.json (shape
    // findings mà audit scanner của repo xuất ra).
    let ledger = r#"{
  "schema": "dependency-delegation-audit/1",
  "generated_at": "2026-09-16T10:36:03Z",
  "tools": ["cargo", "pip", "deno"],
  "summary": {"total": 4, "allowed": 1, "delegated_documented": 3, "violation": 0},
  "findings": [
    {"file": "adapters/game/src/adapter.rs", "line": 104, "tool": "cargo", "op_class": "install", "function": "install", "status": "delegated-documented"},
    {"file": "adapters/game/src/adapter.rs", "line": 199, "tool": "cargo", "op_class": "add", "function": "add", "status": "delegated-documented"},
    {"file": "cli/src/commands/core/install/ai.rs", "line": 40, "tool": "pip", "op_class": "install", "function": "ai_install_command", "status": "delegated-documented"},
    {"file": "adapters/web/src/audit.rs", "line": 101, "tool": "deno", "op_class": "run", "function": "run_audit", "status": "allowed"}
  ]
}"#;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dependencyDelegationAudit.json");
    std::fs::write(&path, ledger).unwrap();

    let entries = load_ownership_ledger(&path).unwrap();
    // (game, delegated, cargo) appears twice → deduped to one entry.
    // (game, delegated, cargo) xuất hiện 2 lần → khử trùng lặp còn 1.
    assert_eq!(entries.len(), 3);

    let game = entries.iter().find(|e| e.core == "game").unwrap();
    assert_eq!(game.owner, "delegated");
    assert_eq!(game.tool.as_deref(), Some("cargo"));
    assert_eq!(game.documented_at, "2026-09-16T10:36:03Z");

    let ai = entries.iter().find(|e| e.core == "ai").unwrap();
    assert_eq!(ai.owner, "delegated");
    assert_eq!(ai.tool.as_deref(), Some("pip"));

    let web = entries.iter().find(|e| e.core == "web").unwrap();
    assert_eq!(web.owner, "mgc-native");
    assert_eq!(web.tool.as_deref(), Some("deno"));
}

#[test]
fn ownership_ledger_fails_closed_on_unknown_status() {
    let ledger = r#"{
  "generated_at": "2026-09-16T10:36:03Z",
  "findings": [
    {"file": "adapters/game/src/adapter.rs", "tool": "cargo", "status": "violation"}
  ]
}"#;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dependencyDelegationAudit.json");
    std::fs::write(&path, ledger).unwrap();

    // Unknown/failed statuses must never be certified into the lock snapshot.
    // Status lạ/hỏng không bao giờ được xác nhận vào ảnh chụp trong lock.
    let err = load_ownership_ledger(&path).unwrap_err();
    assert!(
        err.to_string().contains("unknown delegation ledger status"),
        "{err}"
    );
}

#[test]
fn ownership_gate_requires_registry_or_delegated_provenance() {
    // Case 1: rust pin with neither registry nor provenance → gate fails.
    // Case 1: pin rust thiếu cả registry lẫn provenance → gate chặn.
    let mut lock = Lockfile::new();
    let mut bare = Package::new(
        "syn".to_string(),
        "2.0.0".to_string(),
        "https://static.crates.io/syn".to_string(),
        "blake3-x".to_string(),
    );
    bare.ecosystem = EcosystemTag::Rust;
    lock.add_package(bare);

    match verify_ownership_completeness(&lock) {
        Err(LockfileError::IncompleteProvenance(msg)) => assert!(msg.contains("syn"), "{msg}"),
        other => panic!("expected IncompleteProvenance, got: {other:?}"),
    }

    // Case 2: registry present → gate passes.
    // Case 2: có registry → gate qua.
    let mut lock = Lockfile::new();
    let mut with_registry = Package::new(
        "serde".to_string(),
        "1.0.219".to_string(),
        "u".to_string(),
        "b".to_string(),
    );
    with_registry.ecosystem = EcosystemTag::Rust;
    with_registry.registry = Some("crates://sparse.crates.io".to_string());
    lock.add_package(with_registry);
    assert!(verify_ownership_completeness(&lock).is_ok());

    // Case 3: delegated-tool provenance (no registry) → gate passes.
    // Case 3: provenance delegated-tool (không registry) → gate qua.
    let mut lock = Lockfile::new();
    let mut delegated = Package::new(
        "bevy".to_string(),
        "0.16.0".to_string(),
        String::new(),
        String::new(),
    );
    delegated.ecosystem = EcosystemTag::Rust;
    delegated.provenance = Some(Provenance {
        source_kind: SOURCE_KIND_DELEGATED_TOOL.to_string(),
        tool: Some("cargo 1.85".to_string()),
        imported_from: None,
    });
    lock.add_package(delegated);
    assert!(verify_ownership_completeness(&lock).is_ok());

    // Case 4: v2-era `other` package stays exempt.
    // Case 4: package `other` thời v2 vẫn được miễn.
    let mut lock = Lockfile::new();
    lock.add_package(Package::new(
        "left-pad".to_string(),
        "1.3.0".to_string(),
        "u".to_string(),
        "b".to_string(),
    ));
    assert!(verify_ownership_completeness(&lock).is_ok());
}
