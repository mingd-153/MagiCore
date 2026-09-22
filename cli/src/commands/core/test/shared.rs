use super::*;
use mgc_lockfile::{Lockfile, Package};
use mgc_types::{DependencySpec, Ecosystem, PackageName, VersionRange};

#[test]
fn generic_python_ai_detects_torch_from_pep621_dependencies() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("pyproject.toml"),
        "[project]\ndependencies = [\"torch>=2.8\", \"numpy\"]\n",
    )
    .expect("pyproject");

    assert_eq!(
        detect_ai_runtime(temp.path(), "python-agent"),
        crate::commands::optimizer::runtime_detect::DetectedRuntime::PythonPyTorch
    );
}

#[test]
fn generic_python_ai_without_torch_remains_unknown() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("pyproject.toml"),
        "[project]\ndependencies = [\"tensorflow>=2\"]\n",
    )
    .expect("pyproject");

    assert_eq!(
        detect_ai_runtime(temp.path(), "python-agent"),
        crate::commands::optimizer::runtime_detect::DetectedRuntime::Unknown
    );
}

#[test]
fn v2_lock_matches_manifest_when_locked_version_satisfies_range() {
    let mut manifest = Manifest::new("demo", Ecosystem::Web);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("tailwindcss").unwrap(),
            VersionRange::parse("^4.3.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let mut lock = Lockfile::new();
    lock.add_package(Package::new(
        "tailwindcss".into(),
        "4.3.2".into(),
        "https://registry.example/tailwindcss.tgz".into(),
        "blake3-tailwindcss".into(),
    ));

    assert!(lock_matches_manifest(&lock, &manifest));
}

#[test]
fn v2_lock_rejects_stale_manifest_requirement() {
    let mut manifest = Manifest::new("demo", Ecosystem::Web);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("tailwindcss").unwrap(),
            VersionRange::parse("^5.0.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let mut lock = Lockfile::new();
    lock.add_package(Package::new(
        "tailwindcss".into(),
        "4.3.2".into(),
        "https://registry.example/tailwindcss.tgz".into(),
        "blake3-tailwindcss".into(),
    ));

    assert!(!lock_matches_manifest(&lock, &manifest));
}

#[test]
fn v2_graph_preserves_dependency_edges() {
    let mut lock = Lockfile::new();
    let mut react = Package::new(
        "react".into(),
        "19.2.0".into(),
        "https://registry.example/react.tgz".into(),
        "blake3-react".into(),
    );
    react.add_dependency("scheduler@0.25.0".into());
    lock.add_package(react);
    lock.add_package(Package::new(
        "scheduler".into(),
        "0.25.0".into(),
        "https://registry.example/scheduler.tgz".into(),
        "blake3-scheduler".into(),
    ));

    let graph = graph_from_lockfile(&lock).unwrap();
    assert_eq!(graph.packages.len(), 2);
    assert_eq!(graph.packages[0].deps[0].to_string(), "scheduler@0.25.0");
}

#[test]
fn why_edge_name_strips_version_and_range() {
    // Tên trần từ cạnh lock — hậu tố version/range không tham gia so khớp
    assert_eq!(edge_name("leftpad"), "leftpad");
    assert_eq!(edge_name("leftpad@1.3.0"), "leftpad");
    assert_eq!(edge_name("leftpad@^1.3.0"), "leftpad");
    assert_eq!(edge_name("@scope/pkg"), "@scope/pkg");
    assert_eq!(edge_name("@scope/pkg@1.3.0"), "@scope/pkg");
    assert_eq!(edge_name("  pad-core@2.0.0  "), "pad-core");
}

#[test]
fn rollback_error_carries_both_install_and_restore_failures() {
    // P0: restore failure must propagate INSIDE the returned error —
    // warning-only would hide a half-updated project from CI/API.
    // (Lỗi restore phải nằm trong error trả về — warning là nuốt lỗi.)
    let err = combine_rollback_errors("fetch broke: connection refused", "disk read-only");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("fetch broke: connection refused"),
        "install error must surface: {msg}"
    );
    assert!(
        msg.contains("disk read-only"),
        "restore error must surface: {msg}"
    );
    assert!(
        msg.contains("half-updated"),
        "must flag possible half-updated state: {msg}"
    );
}

#[tokio::test]
async fn interrupted_remove_journal_recovers_manifest_and_lock() {
    // Hermetic crash-recovery test (NO registry): stage a journal, then
    // simulate post-crash drift with a FOREIGN pid — recover must
    // restore the manifest semantically and the lock byte-identical,
    // then clear the journal.
    // (Test phục hồi crash không cần mạng: journal pid lạ + file drift.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = [\"six==1.17.0\", \"attrs==23.1.0\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("mgc.lock"), "LOCK-BEFORE").unwrap();
    let adapter = mgc_lib_adapter::adapter_for(root, None, None)
        .unwrap()
        .expect("pyproject must detect a python lib adapter");
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot = RemoveSnapshot::capture(&manifest, root).unwrap();
    write_remove_journal(root, &["attrs".to_string()], &snapshot).unwrap();
    // Simulate a crash in ANOTHER process: foreign pid + drifted files.
    // (Giả crash tiến trình khác: pid lạ + file đã lệch.)
    let journal_path = root.join(".magicore/journal/remove/journal.json");
    let mut journal: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&journal_path).unwrap()).unwrap();
    journal["pid"] = serde_json::json!(u64::from(std::process::id()) + 1_000_000);
    std::fs::write(&journal_path, serde_json::to_string_pretty(&journal).unwrap()).unwrap();
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\ndependencies = []\n",
    )
    .unwrap();
    std::fs::write(root.join("mgc.lock"), "LOCK-DRIFTED").unwrap();

    recover_interrupted_remove(&adapter, root).await.unwrap();

    let after = adapter.parse_manifest(root).await.unwrap();
    let names = manifest_dep_names(&after);
    assert!(names.contains(&"six".to_string()), "six restored: {names:?}");
    assert!(
        names.contains(&"attrs".to_string()),
        "attrs restored: {names:?}"
    );
    assert_eq!(
        std::fs::read(root.join("mgc.lock")).unwrap(),
        b"LOCK-BEFORE",
        "lock must restore byte-identical"
    );
    assert!(
        !journal_path.exists(),
        "journal must be cleared after recovery"
    );
}
