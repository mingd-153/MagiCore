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
    // Hermetic crash-recovery test (NO registry): stage a journal, run
    // the op's own write + post-image, then "crash" — a fresh process
    // (flag dead, pid ignored) must restore the pre-image manifest
    // digest and the byte-identical lock, then clear the journal.
    // (Test phục hồi crash không cần mạng: pid trên đĩa bị lờ đi.)
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
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    // Simulate the op's own write + crash AFTER it: drift the files,
    // record the post-image (exactly what the op does after writing),
    // then die. A fresh process (flag cleared, pid ignored) must
    // recover to the pre-image.
    // (Giả crash sau khi op tự ghi: drift + post-image, rồi chết.)
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\ndependencies = []\n",
    )
    .unwrap();
    std::fs::write(root.join("mgc.lock"), "LOCK-DRIFTED").unwrap();
    let drifted = adapter.parse_manifest(root).await.unwrap();
    record_post_image(root, &drifted, &write_lock).unwrap();
    let journal_path = root.join(".magicore/journal/dependency-mutation/journal.json");

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
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    // Op "succeeded" elsewhere, then cleanup failed: flip to completed
    // with a FOREIGN pid (as if staged by the dead process)…
    // (Giả op đã xong ở process khác: completed + pid lạ.)
    let journal_path = root.join(".magicore/journal/dependency-mutation/journal.json");
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
    let journal_dir = root.join(".magicore/journal/dependency-mutation");
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
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let journal_parent = root.join(".magicore/journal");
    std::fs::create_dir_all(&journal_parent).unwrap();
    symlink(outside.path(), journal_parent.join("dependency-mutation")).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
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

#[cfg(unix)]
#[tokio::test]
async fn symlinked_journal_file_is_never_followed() {
    // journal.json là symlink trỏ ra ngoài: recovery từ chối, artifact
    // giữ nguyên (Item 2 — đọc sau khi chống, không trước).
    // (Symlinked journal.json is refused, never followed.)
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    // Swap journal.json for a link to an "in_progress" journal outside
    // the project — a naive read-then-validate would restore from it.
    // (Tráo journal.json bằng link ra ngoài — đọc ngây thơ sẽ dính.)
    let journal_path = root.join(".magicore/journal/dependency-mutation/journal.json");
    let outside = tempfile::tempdir().unwrap();
    let evil = outside.path().join("evil.json");
    std::fs::write(
        &evil,
        r#"{"v":1,"pid":999999,"packages":[],"lock_existed":false,"state":"in_progress"}"#,
    )
    .unwrap();
    std::fs::remove_file(&journal_path).unwrap();
    symlink(&evil, &journal_path).unwrap();
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("symlinked journal.json must be refused");
    // Nothing followed the link: project files intact, outside untouched
    // beyond our own setup write.
    // (Không theo link: file project nguyên, ngoài không bị động.)
    assert_eq!(
        std::fs::read(root.join("mgc.lock")).unwrap(),
        b"LOCK-BEFORE"
    );
    let after = adapter.parse_manifest(root).await.unwrap();
    assert_eq!(
        manifest_canonical_digest(&after),
        manifest_canonical_digest(&manifest)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_magicore_parent_is_refused() {
    // .magicore là symlink: acquire lock và staging journal đều từ chối
    // (Item 4 — parent gap đã đóng).
    // (Symlinked .magicore is refused by lock acquire and journal write.)
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.join(".magicore")).unwrap();
    mgc_lockfile::project_lock::ProjectWriteLock::acquire(root, std::time::Duration::from_secs(5))
        .expect_err("symlinked .magicore must refuse the writer lock");
}

#[tokio::test]
async fn user_edit_after_crash_fails_closed() {
    // User sửa manifest sau crash (không khớp pre/post): recovery LỖI,
    // không ghi đè việc của user; artifact giữ nguyên.
    // (Post-crash user edits fail closed — never overwritten.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    let drifted = adapter.parse_manifest(root).await.unwrap();
    record_post_image(root, &drifted, &write_lock).unwrap();
    // A human edits the manifest after the crash (third state).
    // (User sửa tay sau crash — trạng thái thứ ba.)
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\ndependencies = [\"six==1.17.0\", \"attrs==23.1.0\", \"human-edit==1.0.0\"]\n",
    )
    .unwrap();

    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("user edits after crash must fail closed");
    let kept = std::fs::read_to_string(root.join("pyproject.toml")).unwrap();
    assert!(
        kept.contains("human-edit"),
        "user edit must survive the refused recovery:\n{kept}"
    );
}

#[tokio::test]
async fn missing_manifest_backup_fails_closed() {
    // Journal in_progress nhưng mất backup: lỗi, không đoán từ manifest
    // hiện tại (pre-image đã mất).
    // (Missing backup fails closed — the pre-image is gone.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    std::fs::remove_file(root.join(".magicore/journal/dependency-mutation/manifest.json")).unwrap();

    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("missing backup must fail closed");
}

#[tokio::test]
async fn staged_journal_in_project_a_never_blocks_project_b() {
    // P0-2: không còn cờ global — project A giữ journal đang mở, project
    // B vẫn stage/post/finish bình thường trong CÙNG process (MCP,
    // monorepo, test runner). Lock OS riêng từng project nên không chặn nhau.
    // (An open journal in A never affects B in the same process.)
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    python_fixture(dir_a.path());
    python_fixture(dir_b.path());
    let (adapter_a, lock_a) = test_adapter_and_lock(dir_a.path());
    let (adapter_b, lock_b) = test_adapter_and_lock(dir_b.path());
    let manifest_a = adapter_a.parse_manifest(dir_a.path()).await.unwrap();
    let snapshot_a = MutationSnapshot::capture(
        &manifest_a,
        dir_a.path(),
        &lock_a,
        MutationOperation::Remove,
    )
    .unwrap();
    // A stages and HOLDS (guard + journal stay alive).
    // (A stage rồi giữ — không finish.)
    stage_mutation_journal(
        dir_a.path(),
        &adapter_a,
        &["attrs".to_string()],
        &snapshot_a,
        &lock_a,
    )
    .unwrap();
    // B runs a full stage → post → finish cycle unaffected.
    // (B chạy trọn vòng không ảnh hưởng.)
    let manifest_b = adapter_b.parse_manifest(dir_b.path()).await.unwrap();
    let snapshot_b = MutationSnapshot::capture(
        &manifest_b,
        dir_b.path(),
        &lock_b,
        MutationOperation::Remove,
    )
    .unwrap();
    stage_mutation_journal(
        dir_b.path(),
        &adapter_b,
        &["attrs".to_string()],
        &snapshot_b,
        &lock_b,
    )
    .unwrap();
    record_post_image(dir_b.path(), &manifest_b, &lock_b).unwrap();
    finish_mutation_journal(dir_b.path(), &lock_b).unwrap();
    assert!(
        !dir_b
            .path()
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists(),
        "B's journal must clear"
    );
    assert!(
        dir_a
            .path()
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists(),
        "A's held journal must be untouched by B"
    );
    drop(lock_a);
}

#[tokio::test]
async fn aborted_task_leaves_recoverable_journal() {
    // Hủy task giữa mutation (panic/cancel): guard rớt theo future,
    // lock OS tự nhả, journal ở lại cho gateway entry sau (không còn
    // ownership in-memory nào để leak).
    // (Aborted task leaves a recoverable journal, no leaked ownership.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    python_fixture(&root);
    let root_clone = root.clone();
    let handle = tokio::spawn(async move {
        let adapter = mgc_lib_adapter::adapter_for(&root_clone, None, None)
            .unwrap()
            .expect("adapter");
        // Full gateway entry (acquire + recover), then stage, then hang
        // forever mid-mutation — the abort below simulates SIGKILL.
        // (Entry gateway thật rồi treo giữa mutation — abort giả SIGKILL.)
        let guard = begin_dependency_mutation(&adapter, &root_clone, MutationOperation::Remove)
            .await
            .unwrap();
        let manifest = adapter.parse_manifest(&root_clone).await.unwrap();
        let snapshot =
            MutationSnapshot::capture(&manifest, &root_clone, &guard, MutationOperation::Remove)
                .unwrap();
        stage_mutation_journal(
            &root_clone,
            &adapter,
            &["attrs".to_string()],
            &snapshot,
            &guard,
        )
        .unwrap();
        futures_util::future::pending::<()>().await;
        #[allow(unreachable_code)]
        drop(guard);
    });
    // Let the task stage, then abort it mid-mutation.
    // (Cho task stage xong rồi hủy giữa chừng.)
    let mut staged = false;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        if root
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists()
        {
            staged = true;
            break;
        }
    }
    assert!(staged, "task must stage before abort");
    handle.abort();
    let _ = handle.await;

    // A fresh gateway entry acquires (lock auto-released on abort),
    // recovers the staged journal (current == pre: nothing was written),
    // and proceeds — journal gone afterwards.
    // (Entry mới acquire, phục hồi journal dở, rồi đi tiếp.)
    let adapter = mgc_lib_adapter::adapter_for(&root, None, None)
        .unwrap()
        .expect("adapter");
    begin_dependency_mutation(&adapter, &root, MutationOperation::Remove)
        .await
        .expect("fresh entry must acquire and recover after abort");
    assert!(
        !root
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists(),
        "aborted op's journal must be recoverable by the next entry"
    );
}

#[tokio::test]
async fn foreign_core_journal_is_never_restored() {
    // P0-3: journal của core khác (web) đem sang project lib — recovery
    // phải từ chối, không được lấy adapter hiện tại làm authority.
    // (Foreign-core journal fails closed — never restored cross-core.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let journal_dir = root.join(".magicore/journal/dependency-mutation");
    std::fs::create_dir_all(&journal_dir).unwrap();
    let canonical = root.canonicalize().unwrap().display().to_string();
    let journal = serde_json::json!({
        "v": 4,
        "op": "remove",
        "pid": 12345,
        "packages": ["some-web-dep"],
        "lock_existed": true,
        "identity": {
            "core": "app",
            "language": "flutter",
            "format": "pubspec.yaml",
            "relpath": "pubspec.yaml",
        },
        "project_root": canonical,
        "project_id": "00000000-0000-4000-8000-000000000000",
        "pre_digest": [],
        "post_digest": null,
        "state": "in_progress",
    });
    std::fs::write(
        journal_dir.join("journal.json"),
        serde_json::to_string_pretty(&journal).unwrap(),
    )
    .unwrap();
    // This project has a DIFFERENT real identity (its own project.id).
    // (Project này có UUID thật khác — journal ngoại lai.)
    std::fs::write(
        root.join(".magicore/project.id"),
        "11111111-1111-4111-8111-111111111111",
    )
    .unwrap();
    std::fs::write(
        journal_dir.join("journal.json"),
        serde_json::to_string_pretty(&journal).unwrap(),
    )
    .unwrap();
    let err = recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("foreign-core journal must fail closed");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("app") && msg.contains("foreign"),
        "must name the mismatch, not silently restore: {msg}"
    );
    let manifest = std::fs::read_to_string(root.join("pyproject.toml")).unwrap();
    assert!(
        manifest.contains("attrs"),
        "current manifest must be untouched:\n{manifest}"
    );
}

#[tokio::test]
async fn copied_project_journal_recovers_same_lineage() {
    // P1-2: copy cả project (UUID đi cùng) — lineage giống nhau, state
    // giống nhau → recovery thành công đúng đắn (không phải ngoại lai).
    // Khác với project KHÁC UUID (test foreign) luôn bị từ chối.
    // (Copied project shares lineage — recovery succeeds correctly.)
    let dir_a = tempfile::tempdir().unwrap();
    python_fixture(dir_a.path());
    let (adapter_a, lock_a) = test_adapter_and_lock(dir_a.path());
    let manifest = adapter_a.parse_manifest(dir_a.path()).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, dir_a.path(), &lock_a, MutationOperation::Remove)
            .unwrap();
    stage_mutation_journal(
        dir_a.path(),
        &adapter_a,
        &["attrs".to_string()],
        &snapshot,
        &lock_a,
    )
    .unwrap();
    drop(lock_a);
    // Copy the whole tree (journal + project.id included).
    // (Copy toàn bộ cây — journal + UUID đi cùng.)
    let dir_b = tempfile::tempdir().unwrap();
    copy_dir_recursive(dir_a.path(), dir_b.path()).unwrap();
    let (adapter_b, lock_b) = test_adapter_and_lock(dir_b.path());
    recover_interrupted_remove(&adapter_b, dir_b.path(), &lock_b)
        .await
        .unwrap();
    let after = adapter_b.parse_manifest(dir_b.path()).await.unwrap();
    assert_eq!(
        manifest_canonical_digest(&after),
        manifest_canonical_digest(&manifest),
        "copied lineage restores identically"
    );
    assert!(
        !dir_b
            .path()
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists(),
        "journal must clear after recovery"
    );
    drop(lock_b);
}

#[tokio::test]
async fn moved_project_recovers_with_notice() {
    // P1-2: rename/move checkout (UUID đi cùng) — recovery thành công
    // (move hợp lệ, khác với copy-sang-project-lạ).
    // (Moved checkout recovers — same UUID lineage.)
    let outer = tempfile::tempdir().unwrap();
    let root_a = outer.path().join("a");
    std::fs::create_dir_all(&root_a).unwrap();
    python_fixture(&root_a);
    std::fs::write(root_a.join("mgc.lock"), "LOCK-BEFORE").unwrap();
    let (adapter, lock) = test_adapter_and_lock(&root_a);
    let manifest = adapter.parse_manifest(&root_a).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, &root_a, &lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(&root_a, &adapter, &["attrs".to_string()], &snapshot, &lock).unwrap();
    drop(lock);
    let root_b = outer.path().join("b");
    std::fs::rename(&root_a, &root_b).unwrap();

    let (adapter_b, lock_b) = test_adapter_and_lock(&root_b);
    recover_interrupted_remove(&adapter_b, &root_b, &lock_b)
        .await
        .unwrap();
    let after = adapter_b.parse_manifest(&root_b).await.unwrap();
    assert_eq!(
        manifest_canonical_digest(&after),
        manifest_canonical_digest(&manifest)
    );
    drop(lock_b);
}

#[tokio::test]
async fn missing_project_id_fails_closed() {
    // P1-2: có journal nhưng mất project.id (thư mục bị thay/khuyết) —
    // fail-closed, không đoán lineage.
    // (Missing project.id with a staged journal fails closed.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    std::fs::remove_file(root.join(".magicore/project.id")).unwrap();
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("missing project.id must fail closed");
    drop(write_lock);
}

#[test]
fn v1_journal_schema_fails_closed_explicitly() {
    // P1-3: schema v1 (chưa từng public — RC branch only) bị từ chối rõ
    // ràng, không migrate thầm, không đoán.
    // (v1 journals fail with an explicit schema error.)
    let raw = r#"{"v":1,"op":"remove","pid":1,"packages":[],"lock_existed":false,"pre_digest":[],"post_digest":null,"state":"in_progress"}"#;
    let err = parse_mutation_journal(raw, std::path::Path::new("journal.json")).unwrap_err();
    assert!(
        format!("{err:#}").contains("unsupported mutation journal schema v1"),
        "must name the unsupported schema: {err:#}"
    );
}

fn copy_dir_recursive(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn traversal_packages_field_is_inert() {
    // Journal packages chứa traversal không thể thoát ra ngoài: field
    // này không bao giờ thành path (chỉ diagnostic). Recovery vẫn chạy
    // bình thường với pre-image.
    // (Traversal-looking packages cannot escape — the field is inert.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["../../evil".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    let drifted = adapter.parse_manifest(root).await.unwrap();
    record_post_image(root, &drifted, &write_lock).unwrap();
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .unwrap();
    assert!(
        !root.parent().unwrap().join("evil").exists(),
        "no traversal side effect may exist"
    );
    assert!(
        !root
            .join(".magicore/journal/dependency-mutation/journal.json")
            .exists(),
        "journal must clear after normal recovery"
    );
}

#[tokio::test]
async fn legacy_journal_path_is_adopted_then_recovered() {
    // P1-3: journal ở path cũ (journal/remove) được rename nguyên tử
    // sang path mới rồi phục hồi bình thường — không mất, không double.
    // (Legacy journal is adopted atomically, then recovered.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    // Rewind to the legacy layout (as an older binary left it).
    // (Giả journal của binary cũ ở path legacy.)
    let legacy = root.join(".magicore/journal/remove");
    let current = root.join(".magicore/journal/dependency-mutation");
    std::fs::rename(&current, &legacy).unwrap();

    adopt_legacy_journal(root, &write_lock).unwrap();
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .unwrap();
    assert!(!legacy.exists(), "legacy dir must be adopted away");
    assert!(
        !current.join("journal.json").exists(),
        "adopted journal must be recovered and cleared (current == pre)"
    );
    let after = adapter.parse_manifest(root).await.unwrap();
    assert_eq!(
        manifest_canonical_digest(&after),
        manifest_canonical_digest(&manifest)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_legacy_marker_fails_closed() {
    // P1-3: marker legacy là symlink ra ngoài — adopt dời cả thư mục
    // nhưng recovery từ chối theo link, không đọc file ngoài.
    // (Symlinked legacy marker is refused at recovery, never followed.)
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let outside = tempfile::tempdir().unwrap();
    let evil = outside.path().join("evil.json");
    std::fs::write(&evil, r#"{"v":2}"#).unwrap();
    let legacy = root.join(".magicore/journal/remove");
    std::fs::create_dir_all(&legacy).unwrap();
    symlink(&evil, legacy.join("journal.json")).unwrap();

    // No pre-held guard here: begin() acquires its own (holding one
    // across the call would LockBusy against itself, not test recovery).
    // (Không giữ lock trước — begin tự acquire.)
    let adapter = mgc_lib_adapter::adapter_for(root, None, None)
        .unwrap()
        .expect("adapter");
    begin_dependency_mutation(&adapter, root, MutationOperation::Remove)
        .await
        .expect_err("symlinked legacy marker must fail closed");
    assert_eq!(
        std::fs::read(&evil).unwrap(),
        b"{\"v\":2}",
        "outside file must be untouched"
    );
}

#[tokio::test]
async fn adopt_refuses_when_current_dir_has_garbage() {
    // P1-3: current dir tồn tại nhưng không marker + legacy có journal —
    // rename thất bại → lỗi fail-closed, không merge, không mất.
    // (Adopt refuses when the current dir is occupied without a marker.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    let manifest = adapter.parse_manifest(root).await.unwrap();
    let snapshot =
        MutationSnapshot::capture(&manifest, root, &write_lock, MutationOperation::Remove).unwrap();
    stage_mutation_journal(
        root,
        &adapter,
        &["attrs".to_string()],
        &snapshot,
        &write_lock,
    )
    .unwrap();
    let legacy = root.join(".magicore/journal/remove");
    let current = root.join(".magicore/journal/dependency-mutation");
    std::fs::rename(&current, &legacy).unwrap();
    // Occupy the current dir with marker-less garbage.
    // (Chiếm dir mới bằng rác không marker.)
    std::fs::create_dir_all(&current).unwrap();
    std::fs::write(current.join("junk.txt"), "junk").unwrap();
    // Release the staging guard before re-acquiring below (same-process
    // flock is not re-entrant).
    // (Nhả guard staging trước khi acquire lại.)
    drop(write_lock);

    // begin acquires then adopt fails → whole entry fails closed.
    // (Adopt lỗi → entry lỗi, journal cũ còn nguyên.)
    let lock_result = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        root,
        std::time::Duration::from_secs(5),
    );
    assert!(lock_result.is_ok(), "lock itself must acquire");
    let guard = lock_result.unwrap();
    adopt_legacy_journal(root, &guard).expect_err("occupied current dir must refuse adoption");
    assert!(
        legacy.join("journal.json").exists(),
        "legacy journal must survive the refused adoption"
    );
    drop(guard);
}

/// Craft a v4 journal with an explicit owner identity (no staging
/// needed — identity is verified before any backup is touched). A
/// matching project.id is written so the check under test is the
/// IDENTITY, not the lineage.
/// (Dựng journal v4 + project.id khớp — test identity thuần.)
fn craft_identity_journal(
    root: &std::path::Path,
    core: &str,
    language: &str,
    format: &str,
    relpath: &str,
) {
    const FIXED_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    std::fs::write(root.join(".magicore/project.id"), FIXED_ID).unwrap();
    let dir = root.join(".magicore/journal/dependency-mutation");
    std::fs::create_dir_all(&dir).unwrap();
    let canonical = root.canonicalize().unwrap().display().to_string();
    let journal = serde_json::json!({
        "v": 4,
        "op": "remove",
        "pid": 4242,
        "packages": ["x"],
        "lock_existed": false,
        "identity": {
            "core": core,
            "language": language,
            "format": format,
            "relpath": relpath,
        },
        "project_root": canonical,
        "project_id": FIXED_ID,
        "pre_digest": [],
        "post_digest": null,
        "state": "in_progress",
    });
    std::fs::write(
        dir.join("journal.json"),
        serde_json::to_string_pretty(&journal).unwrap(),
    )
    .unwrap();
}

#[cfg(feature = "game")]
#[tokio::test]
async fn game_bevy_journal_rejected_by_godot_command() {
    // Blocker 1: cùng adapter game, khác engine — bevy journal qua tay
    // godot command phải fail (core giống nhau chưa đủ).
    // (Same adapter, different engine — must mismatch.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("project.godot"), "; godot\n").unwrap();
    let adapter =
        mgc_game_adapter::adapter_for(root).expect("project.godot must detect a game adapter");
    let write_lock = ProjectWriteLock::acquire(root, std::time::Duration::from_secs(30)).unwrap();
    craft_identity_journal(root, "game", "bevy", "Cargo.toml", "Cargo.toml");
    let err = recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("bevy journal via godot command must fail closed");
    assert!(
        format!("{err:#}").contains("bevy"),
        "must name the staged lane: {err:#}"
    );
    drop(write_lock);
}

#[cfg(feature = "iot")]
#[tokio::test]
async fn iot_esp32_journal_rejected_by_platformio_command() {
    // Blocker 1: cùng adapter iot, khác framework — esp32 journal qua
    // tay platformio command phải fail.
    // (Same adapter, different framework — must mismatch.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("platformio.ini"), "[env:test]\n").unwrap();
    let adapter =
        mgc_iot_adapter::adapter_for(root).expect("platformio must detect an iot adapter");
    let write_lock = ProjectWriteLock::acquire(root, std::time::Duration::from_secs(30)).unwrap();
    craft_identity_journal(root, "iot", "esp32-rust", "Cargo.toml", "Cargo.toml");
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("esp32 journal via pio command must fail closed");
    drop(write_lock);
}

#[cfg(feature = "clo")]
#[tokio::test]
async fn cloud_terraform_journal_rejected_by_cdk_command() {
    // Blocker 1: cùng adapter cloud, khác type — terraform journal qua
    // tay cdk command phải fail.
    // (Same adapter, different cloud type — must mismatch.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("package.json"),
        r#"{ "name": "infra", "dependencies": { "aws-cdk-lib": "^2.0.0" } }"#,
    )
    .unwrap();
    let adapter = mgc_cloud_adapter::adapter_for(root)
        .expect("adapter_for must not error")
        .expect("cdk package.json must detect a cloud adapter");
    let write_lock = ProjectWriteLock::acquire(root, std::time::Duration::from_secs(30)).unwrap();
    craft_identity_journal(
        root,
        "cloud",
        "terraform",
        ".terraform.lock.hcl",
        ".terraform.lock.hcl",
    );
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("terraform journal via cdk command must fail closed");
    drop(write_lock);
}

#[tokio::test]
async fn relpath_participates_in_identity() {
    // Blocker 1: cùng core/language/format nhưng khác relpath vẫn
    // mismatch — relpath có tham gia so sánh (không phải trường trang trí).
    // (relpath mismatch fails even when everything else matches.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    python_fixture(root);
    let (adapter, write_lock) = test_adapter_and_lock(root);
    craft_identity_journal(root, "lib", "python", "pyproject.toml", "other.toml");
    recover_interrupted_remove(&adapter, root, &write_lock)
        .await
        .expect_err("relpath mismatch must fail closed");
    drop(write_lock);
}

#[cfg(feature = "game")]
#[tokio::test]
async fn optimizer_hook_conflicting_value_fails_closed() {
    // Hook optimizer gặp dependency cùng tên khác giá trị: LỖI chứ
    // không ghi đè mù (idempotent chỉ khi giá trị khớp).
    // (Conflicting user value fails closed — never overwritten.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"g\"\nversion = \"0.1.0\"\n\n[dependencies]\nmgc-optimizer = { path = \"./custom\" }\n",
    )
    .unwrap();
    game_hook_optimizer_dep(root)
        .await
        .expect_err("conflicting mgc-optimizer value must fail closed");
    let kept = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(
        kept.contains("./custom"),
        "user value must survive:\n{kept}"
    );
}

#[cfg(feature = "game")]
#[tokio::test]
async fn optimizer_hook_inserts_and_converges() {
    // Hook chèn dep thiếu (atomic), chạy lại hội tụ cùng bytes.
    // (Missing dep inserted atomically; re-run converges.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"g\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
    )
    .unwrap();
    game_hook_optimizer_dep(root).await.unwrap();
    let once = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(once.contains("./optimizer"), "dep must land:\n{once}");
    game_hook_optimizer_dep(root).await.unwrap();
    let twice = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert_eq!(once, twice, "re-run must converge byte-identical");
}
