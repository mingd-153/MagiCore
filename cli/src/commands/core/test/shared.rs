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
    let write_lock = ProjectWriteLock::acquire(root, std::time::Duration::from_secs(30)).unwrap();
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot = RemoveSnapshot::capture(&manifest, root, &write_lock).unwrap();
    write_remove_journal(root, &["attrs".to_string()], &snapshot, &write_lock).unwrap();
    // Simulate a crash in ANOTHER process: foreign pid + drifted files.
    // (Giả crash tiến trình khác: pid lạ + file đã lệch.)
    let journal_path = root.join(".magicore/journal/remove/journal.json");
    let mut journal: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&journal_path).unwrap()).unwrap();
    journal["pid"] = serde_json::json!(u64::from(std::process::id()) + 1_000_000);
    std::fs::write(
        &journal_path,
        serde_json::to_string_pretty(&journal).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\ndependencies = []\n",
    )
    .unwrap();
    std::fs::write(root.join("mgc.lock"), "LOCK-DRIFTED").unwrap();

    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .unwrap();

    let after = adapter.parse_manifest(root).await.unwrap();
    assert_eq!(
        manifest_canonical_digest(&after),
        manifest_canonical_digest(&manifest),
        "manifest must restore to the same canonical digest (project, groups, ranges, flags)"
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

fn python_fixture(root: &std::path::Path) {
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = [\"six==1.17.0\", \"attrs==23.1.0\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("mgc.lock"), "LOCK-BEFORE").unwrap();
}

fn test_adapter_and_lock(
    root: &std::path::Path,
) -> (
    mgc_lib_adapter::LibAdapter,
    mgc_lockfile::project_lock::ProjectWriteLock,
) {
    let adapter = mgc_lib_adapter::adapter_for(root, None, None)
        .unwrap()
        .expect("pyproject must detect a python lib adapter");
    let lock = ProjectWriteLock::acquire(root, std::time::Duration::from_secs(30)).unwrap();
    (adapter, lock)
}

#[tokio::test]
async fn completed_journal_deletes_without_restoring() {
    // P0-A: a stale COMPLETED journal (success + failed cleanup) must
    // NEVER roll back — recovery only deletes it, files stay untouched.
    // (Journal completed sót: chỉ xóa, không restore.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot = RemoveSnapshot::capture(&manifest, root, &write_lock).unwrap();
    write_remove_journal(root, &["attrs".to_string()], &snapshot, &write_lock).unwrap();
    // Op "succeeded" elsewhere, then cleanup failed: flip to completed
    // with a FOREIGN pid (as if staged by the dead process)…
    // (Giả op đã xong ở process khác: completed + pid lạ.)
    let journal_path = root.join(".magicore/journal/remove/journal.json");
    let mut journal: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&journal_path).unwrap()).unwrap();
    journal["pid"] = serde_json::json!(u64::from(std::process::id()) + 1_000_000);
    journal["state"] = serde_json::json!("completed");
    std::fs::write(
        &journal_path,
        serde_json::to_string_pretty(&journal).unwrap(),
    )
    .unwrap();
    // …then the world drifts (new successful state). Recovery must NOT
    // resurrect the removed dep.
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\ndependencies = [\"six==1.17.0\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("mgc.lock"), "LOCK-NEW-STATE").unwrap();

    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .unwrap();

    assert!(!journal_path.exists(), "completed journal must be deleted");
    let after = std::fs::read_to_string(root.join("pyproject.toml")).unwrap();
    assert!(
        !after.contains("attrs"),
        "removed dep must NOT be resurrected: {after}"
    );
    assert_eq!(
        std::fs::read(root.join("mgc.lock")).unwrap(),
        b"LOCK-NEW-STATE",
        "lock must stay untouched"
    );
}

#[tokio::test]
async fn corrupt_journal_fails_closed() {
    // Journal hỏng: không đoán — lỗi fail-closed.
    // (Corrupt journal fails closed, never guesses.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let journal_dir = root.join(".magicore/journal/remove");
    std::fs::create_dir_all(&journal_dir).unwrap();
    std::fs::write(journal_dir.join("journal.json"), "{not json").unwrap();
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("corrupt journal must fail closed");
}

#[tokio::test]
async fn missing_journal_is_a_noop() {
    // Không có journal: không làm gì.
    // (Missing journal is a no-op.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(root.join("mgc.lock")).unwrap(),
        b"LOCK-BEFORE"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_journal_dir_is_refused() {
    // Journal dir là symlink: từ chối, không ghi ra ngoài project.
    // (Symlinked journal dir is refused, never followed.)
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot = RemoveSnapshot::capture(&manifest, root, &write_lock).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let journal_parent = root.join(".magicore/journal");
    std::fs::create_dir_all(&journal_parent).unwrap();
    symlink(outside.path(), journal_parent.join("remove")).unwrap();
    write_remove_journal(root, &["attrs".to_string()], &snapshot, &write_lock)
        .expect_err("symlinked journal dir must be refused");
    assert!(
        !outside.path().join("journal.json").exists(),
        "nothing must be written outside the project"
    );
}

#[test]
fn canonical_digest_distinguishes_ranges_groups_and_project() {
    // P0-D: digest phải bắt range/group/project — không chỉ tên.
    // (Digest must catch range/group/project drift, not just names.)
    use mgc_types::{DependencySpec, Ecosystem, Manifest, PackageName, VersionRange};
    let mut base = Manifest::new("m", Ecosystem::Lib);
    base.add_dep(
        DependencySpec::new(
            PackageName::new("six").unwrap(),
            VersionRange::parse("==1.17.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let digest = manifest_canonical_digest(&base);
    let mut bumped = base.clone();
    bumped.dependencies[0].range = VersionRange::parse("==1.18.0").unwrap();
    assert_ne!(
        manifest_canonical_digest(&bumped),
        digest,
        "range change must alter the digest"
    );
    let mut regrouped = base.clone();
    let spec = regrouped.dependencies.pop().unwrap();
    regrouped.dev_dependencies.push(spec);
    assert_ne!(
        manifest_canonical_digest(&regrouped),
        digest,
        "group change must alter the digest"
    );
    let mut renamed = base.clone();
    renamed.name = "other".to_string();
    assert_ne!(
        manifest_canonical_digest(&renamed),
        digest,
        "project rename must alter the digest"
    );
    assert_eq!(
        manifest_canonical_digest(&base.clone()),
        digest,
        "identical manifests share the digest"
    );
}
