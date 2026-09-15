#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for cache command (clean, prune, stats logic)

use super::*;

#[test]
fn clean_all_does_not_include_build_cache() {
    let entries = cache_entries(
        CacheTarget::All,
        None,
        CacheAction::Clean.includes_build_target(CacheTarget::All),
    )
    .unwrap();
    assert!(
        entries.iter().all(|entry| entry.label != "build"),
        "clean --target all must not delete Rust build artifacts implicitly"
    );
}

#[test]
fn clean_build_includes_build_cache_explicitly() {
    let entries = cache_entries(
        CacheTarget::Build,
        None,
        CacheAction::Clean.includes_build_target(CacheTarget::Build),
    )
    .unwrap();
    assert!(
        entries.iter().any(|entry| entry.label == "build"),
        "clean --target build should include build cache explicitly"
    );
}

#[test]
fn finds_workspace_target_from_nested_crate() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"cli\"]\n",
    )
    .unwrap();
    let nested = root.path().join("cli").join("src");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        root.path().join("cli").join("Cargo.toml"),
        "[package]\nname = \"cli\"\n",
    )
    .unwrap();
    assert_eq!(
        find_cargo_workspace_root(&nested).unwrap(),
        root.path().to_path_buf()
    );
}

#[test]
fn web_shared_prune_keeps_pinned_and_removes_unpinned_package_roots() {
    let cache = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let pinned = cache
        .path()
        .join("packages")
        .join("react")
        .join("18.2.0-demo")
        .join("package");
    let unpinned = cache
        .path()
        .join("packages")
        .join("zod")
        .join("3.22.4-demo")
        .join("package");
    std::fs::create_dir_all(&pinned).unwrap();
    std::fs::create_dir_all(&unpinned).unwrap();
    std::fs::write(pinned.join(".magicore-package-root.json"), "{}").unwrap();
    std::fs::write(pinned.join("index.js"), "react").unwrap();
    std::fs::write(unpinned.join(".magicore-package-root.json"), "{}").unwrap();
    std::fs::write(unpinned.join("index.js"), "zod").unwrap();
    let refs = cache.path().join("refs").join("projects");
    std::fs::create_dir_all(&refs).unwrap();
    std::fs::write(
        refs.join("demo.json"),
        serde_json::json!({
            "schema_version": 1,
            "project_root": project.path().canonicalize().unwrap().to_string_lossy(),
            "updated_at": 1,
            "package_roots": [
                pinned.canonicalize().unwrap().to_string_lossy()
            ]
        })
        .to_string(),
    )
    .unwrap();
    let removed = prune_web_shared_unpinned_package_roots(cache.path()).unwrap();
    assert_eq!(removed, 1);
    assert!(pinned.join("index.js").exists());
    assert!(!unpinned.exists());
    let stats = web_shared_cache_stats(cache.path());
    assert_eq!(stats.pinned_package_roots, 1);
    assert_eq!(stats.unpinned_package_roots, 0);
}

#[test]
fn web_project_cache_stats_reports_cache_breakdown() {
    let cache = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(cache.path().join("cas")).unwrap();
    std::fs::create_dir_all(cache.path().join("cache")).unwrap();
    std::fs::create_dir_all(cache.path().join("resolutions")).unwrap();
    std::fs::write(cache.path().join("cas").join("blob"), [0u8; 4]).unwrap();
    std::fs::write(cache.path().join("cache").join("tarball.tgz"), [0u8; 3]).unwrap();
    std::fs::write(
        cache.path().join("resolutions").join("graph.json"),
        [0u8; 2],
    )
    .unwrap();
    let stats = web_project_cache_stats(cache.path());
    assert_eq!(
        stats,
        WebProjectCacheStats {
            cas_bytes: 4,
            tarball_bytes: 3,
            resolution_bytes: 2,
        }
    );
}

#[test]
fn web_project_prune_removes_only_unclaimed_cas_files() {
    let root = tempfile::tempdir().unwrap();
    let web = root.path().join(".magicore").join("cache").join("web");
    let cas_blob = web.join("cas").join("ab").join("live");
    let cas_orphan = web.join("cas").join("cd").join("orphan");
    let tarball = web.join("cache").join("pkg").join("1.0.0.tgz");
    let resolution = web.join("resolutions").join("graph.json");
    std::fs::create_dir_all(cas_blob.parent().unwrap()).unwrap();
    std::fs::create_dir_all(cas_orphan.parent().unwrap()).unwrap();
    std::fs::create_dir_all(tarball.parent().unwrap()).unwrap();
    std::fs::create_dir_all(resolution.parent().unwrap()).unwrap();
    std::fs::write(&cas_blob, b"live").unwrap();
    std::fs::write(&cas_orphan, b"orphan").unwrap();
    std::fs::write(&tarball, b"tarball").unwrap();
    std::fs::write(&resolution, b"resolution").unwrap();
    // The blob is LIVE via a DB refcount claim, NOT via nlink: exports are
    // independent copies since the hardlink removal, so a live blob has
    // nlink == 1 — only the refcount can vouch for it (P0-C).
    // Blob SỐNG qua claim refcount trong DB, KHÔNG qua nlink: export là
    // bản sao độc lập từ khi bỏ hardlink nên blob sống có nlink == 1 — chỉ
    // refcount mới bảo chứng được nó (P0-C).
    let db = mgc_store::Database::open(&web.join("store.db")).unwrap();
    let token = db.begin_cas_generation("/proj/demo").unwrap();
    db.cas_claim("/proj/demo", token, "live").unwrap();
    let dry_run = prune_web_project_cache(&web, true).unwrap();
    assert_eq!(
        dry_run,
        WebProjectPruneStats {
            cas_files: 1,
            tarball_files: 1,
            resolution_files: 1,
        }
    );
    assert!(cas_blob.exists());
    assert!(cas_orphan.exists());
    assert!(tarball.exists());
    assert!(resolution.exists());
    let pruned = prune_web_project_cache(&web, false).unwrap();
    assert_eq!(pruned, dry_run);
    assert!(cas_blob.exists());
    assert!(!cas_orphan.exists());
    assert!(!tarball.exists());
    assert!(!resolution.exists());
}

#[test]
fn web_project_prune_keeps_refcount_claimed_cas_blobs() {
    let root = tempfile::tempdir().unwrap();
    let web = root.path().join(".magicore").join("cache").join("web");
    let claimed_blob = web.join("cas").join("ab").join("claimed-hash");
    let orphan_blob = web.join("cas").join("cd").join("orphan-hash");
    std::fs::create_dir_all(claimed_blob.parent().unwrap()).unwrap();
    std::fs::create_dir_all(orphan_blob.parent().unwrap()).unwrap();
    std::fs::write(&claimed_blob, b"claimed").unwrap();
    std::fs::write(&orphan_blob, b"orphan").unwrap();
    let db = mgc_store::Database::open(&web.join("store.db")).unwrap();
    let token = db.begin_cas_generation("/proj/demo").unwrap();
    db.cas_claim("/proj/demo", token, "claimed-hash").unwrap();
    let pruned = prune_web_project_cache(&web, false).unwrap();
    assert_eq!(pruned.cas_files, 1);
    assert!(claimed_blob.exists());
    assert!(!orphan_blob.exists());
}

#[test]
fn web_project_prune_crashed_install_keeps_previous_generation_claims() {
    let root = tempfile::tempdir().unwrap();
    let web = root.path().join(".magicore").join("cache").join("web");
    let blob = web.join("cas").join("ab").join("kept-hash");
    std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
    std::fs::write(&blob, b"kept").unwrap();
    let db = mgc_store::Database::open(&web.join("store.db")).unwrap();
    // Baseline: a previous install claimed the blob (promoted).
    // (Baseline: install trước đã claim blob này (đã promote).)
    let t0 = db.begin_cas_generation("/proj/demo").unwrap();
    db.cas_claim("/proj/demo", t0, "kept-hash").unwrap();
    db.promote_cas_generation("/proj/demo", t0).unwrap();
    // A NEW install crashes mid-flight: it began a staging generation but
    // never promoted — the OLD claims must still protect the blob from
    // prune (P0-A: a staging generation, dead or alive, keeps protecting).
    // (Install MỚI crash giữa chừng: đã begin một staging generation
    // nhưng chưa promote — claim CŨ vẫn phải bảo vệ blob khỏi prune
    // (P0-A: staging generation, chết hay sống, đều tiếp tục bảo vệ).)
    let t_crashed = db.begin_cas_generation("/proj/demo").unwrap();
    assert_ne!(t_crashed, t0, "each install owns a distinct token");
    let pruned = prune_web_project_cache(&web, false).unwrap();
    assert_eq!(pruned.cas_files, 0);
    assert!(
        blob.exists(),
        "un-promoted generation must not orphan the previous refset (P0-C)"
    );
    // The crashed staging generation's own ABORT retires IT (and only
    // it). The baseline's claim survives — the v3 contract: abort of a
    // staging token NEVER touches a promoted generation's claims (the
    // round-10 abort ate them; Gate 11-A P0-3 pins the fix).
    // (ABORT của chính staging generation đứt nghỉ hưu NÓ (và chỉ nó).
    // Claim của baseline sống sót — hợp đồng v3: abort token staging
    // KHÔNG BAO GIỜ đụng claim của generation đã promote (abort vòng-10
    // từng ăn chúng; Gate 11-A P0-3 ghim bản sửa).)
    db.abort_cas_generation("/proj/demo", t_crashed).unwrap();
    let pruned = prune_web_project_cache(&web, false).unwrap();
    assert_eq!(
        pruned.cas_files, 0,
        "the PROMOTED baseline claim must keep the blob after the crashed install's abort"
    );
    assert!(blob.exists());
    // And the v3 hard gate: aborting the PROMOTED token is a typed error,
    // the claims untouched.
    // (Và cổng cứng v3: abort token ĐÃ PROMOTE là lỗi có type, claim
    // nguyên vẹn.)
    assert!(matches!(
        db.abort_cas_generation("/proj/demo", t0),
        Err(mgc_store::CasGenerationError::AlreadyPromoted { .. })
    ));
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["kept-hash".to_string()]);
}

#[test]
fn web_project_prune_corrupt_db_refuses_instead_of_nlink_fallback() {
    let root = tempfile::tempdir().unwrap();
    let web = root.path().join(".magicore").join("cache").join("web");
    let blob = web.join("cas").join("ab").join("blob-hash");
    std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
    std::fs::write(&blob, b"blob").unwrap();
    std::fs::write(web.join("store.db"), b"not a sqlite db").unwrap();
    // P0-C fail-closed contract: with the DB unreadable, prune REFUSES —
    // it must never fall back to nlink (exports are independent copies, so
    // live blobs also have nlink == 1 and would be deleted as orphans).
    // Hợp đồng fail-closed P0-C: DB không đọc được thì prune TỪ CHỐI —
    // tuyệt đối không fallback về nlink (export là bản sao độc lập nên
    // blob sống cũng nlink == 1 và sẽ bị xóa như orphan).
    let result = prune_web_project_cache(&web, false);
    assert!(
        result.is_err(),
        "prune must refuse when store.db is unreadable (P0-C fail-closed)"
    );
    assert!(blob.exists(), "refused prune must not delete anything");
}

#[test]
fn web_project_prune_missing_db_refuses() {
    let root = tempfile::tempdir().unwrap();
    let web = root.path().join(".magicore").join("cache").join("web");
    let orphan = web.join("cas").join("ab").join("orphan-hash");
    std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    std::fs::write(&orphan, b"orphan").unwrap();
    // No store.db at all — prune must refuse rather than guess (P0-C).
    // (Hoàn toàn không có store.db — prune phải từ chối thay vì đoán (P0-C).)
    let result = prune_web_project_cache(&web, false);
    assert!(
        result.is_err(),
        "prune must refuse when store.db is missing"
    );
    assert!(orphan.exists());
}

#[test]
fn generic_core_prune_removes_unlinked_files() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join(".magicore").join("cache").join("ai");
    let stale = cache.join("models").join("stale.bin");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::write(&stale, b"stale").unwrap();
    let dry_run = prune_generic_cache(&cache, true).unwrap();
    assert_eq!(dry_run.files, 1);
    assert!(stale.exists());
    let pruned = prune_generic_cache(&cache, false).unwrap();
    assert_eq!(pruned, dry_run);
    assert!(!stale.exists());
}

#[test]
fn generic_core_prune_keeps_externally_hardlinked_files() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join(".magicore").join("cache").join("lib");
    let live = cache.join("cas").join("live.bin");
    let link = root.path().join("project-live.bin");
    std::fs::create_dir_all(live.parent().unwrap()).unwrap();
    std::fs::write(&live, b"live").unwrap();
    std::fs::hard_link(&live, &link).unwrap();
    let pruned = prune_generic_cache(&cache, false).unwrap();
    assert_eq!(pruned.files, 0);
    assert!(live.exists());
    assert!(link.exists());
}
