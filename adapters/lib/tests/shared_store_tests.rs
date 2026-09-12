#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Shared-store cross-project reuse proof (B-series, 2026-09-12).
//! Chứng minh tái sử dụng chéo project của store chia sẻ.
//!
//! These tests run the REAL cargo/uv toolchain against the REAL shared
//! store (~/.magicore/store/...): two projects with the SAME dependency
//! must not download the dependency twice — the second project's
//! install must show the store growing by ~0 bytes (everything served
//! from the shared store). Network + toolchain required, so they are
//! #[ignore]-gated and run explicitly in CI (release-binary E2E lane
//! + local: cargo test -p mgc-lib-adapter -- --ignored).
//!
//! Các test này chạy cargo/uv THẬT với store chia sẻ THẬT
//! (~/.magicore/store/...): hai project cùng dependency phải không tải
//! dependency hai lần — install của project thứ hai phải cho store
//! tăng ~0 byte (mọi thứ lấy từ store chung). Cần mạng + toolchain
//! nên đánh #[ignore] và chạy tường minh trong CI.

use mgc_lib_adapter::install::shared_store::{SharedStoreRun, dir_size_bytes};

/// Two rust projects sharing ONE dependency: the second fetch must be
/// served entirely by the shared store (delta ≈ 0, reused > 0).
/// Hai project rust chia sẻ MỘT dependency: lần fetch thứ hai phải do
/// store chung phục vụ hoàn toàn (delta ≈ 0, reused > 0).
#[test]
#[ignore = "network + cargo toolchain — run in CI shared-store lane"]
fn shared_cargo_store_reuses_bytes_across_projects() {
    let root = std::env::temp_dir().join(format!(
        "mgc-shared-store-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));
    std::fs::create_dir_all(&root).unwrap();

    for proj in ["proj-a", "proj-b"] {
        let dir = root.join(proj);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"store-proof\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
             [dependencies]\nserde_json = \"1\"\n\n[workspace]\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join("lib.rs"), "").unwrap();
    }

    // Project A: cold — the shared store grows by the dep byte set.
    // (`--locked` needs a pre-existing Cargo.lock; a fresh scaffold
    // resolves on first fetch — the lock pin is not what this test
    // proves, the SHARED BYTES are.)
    // Project A: lạnh — store chung tăng đúng bộ byte dependency.
    // (`--locked` cần Cargo.lock sẵn có; scaffold mới resolve ở lần
    // fetch đầu — pin lock không phải điều test này chứng minh, BYTE
    // CHIA SẺ mới là.)
    let run_a = SharedStoreRun::cargo().unwrap();
    let status_a = std::process::Command::new("cargo")
        .args(["fetch"])
        .current_dir(root.join("proj-a"))
        .envs(run_a.env_vars())
        .status()
        .unwrap();
    assert!(status_a.success(), "cargo fetch (proj-a) failed");
    let (reused_a, _, mode_a) = run_a.finish();
    assert_eq!(
        mode_a as i32,
        mgc_types::adapter::InstallCacheMode::MgCStore as i32
    );

    let store_size_after_a = dir_size_bytes(&SharedStoreRun::cargo().unwrap().cache_root);

    // Project B: warm — SAME serde_json set, the store must NOT grow;
    // every byte came from the shared store (reused > 0).
    // Project B: ấm — cùng tập serde_json, store KHÔNG được tăng;
    // mọi byte đến từ store chung (reused > 0).
    let run_b = SharedStoreRun::cargo().unwrap();
    let status_b = std::process::Command::new("cargo")
        .args(["fetch"])
        .current_dir(root.join("proj-b"))
        .envs(run_b.env_vars())
        .status()
        .unwrap();
    assert!(status_b.success(), "cargo fetch (proj-b) failed");
    let (reused_b, _, mode_b) = run_b.finish();
    assert_eq!(
        mode_b as i32,
        mgc_types::adapter::InstallCacheMode::MgCStore as i32
    );

    let store_size_after_b = dir_size_bytes(&SharedStoreRun::cargo().unwrap().cache_root);

    // The proof: B added (almost) nothing — its bytes came from A's
    // download through the SHARED root (cargo registry cache keys are
    // content-global: same crate version = same cache entry).
    // Bằng chứng: B gần như không thêm gì — byte của B là lần tải của
    // A đi qua gốc CHUNG (khóa cache registry cargo là nội dung toàn
    // cục: cùng phiên bản crate = cùng entry cache).
    assert!(
        reused_b > 0,
        "warm project must report reused bytes from the shared store (got {reused_b})"
    );
    let growth_b = store_size_after_b.saturating_sub(store_size_after_a);
    assert!(
        growth_b < 1024 * 1024,
        "second project grew the shared store by {growth_b} bytes — cross-project reuse broken"
    );
    // Silence unused warning for reused_a on cold run (it may be 0).
    // Im lặng cảnh báo unused cho reused_a ở lượt lạnh (có thể = 0).
    let _ = reused_a;

    std::fs::remove_dir_all(&root).ok();
}

/// The pypi shared run points uv/pip caches at the same directory as
/// the lib/python lane and the ai core — one root, many lanes.
/// Lượt pypi dùng chung trỏ cache uv/pip về cùng thư mục với lane
/// lib/python và core ai — một gốc, nhiều lane.
#[test]
fn pypi_shared_root_is_single_and_pinned() {
    let run = SharedStoreRun::pypi().unwrap();
    let home = dirs::home_dir().unwrap();
    assert_eq!(
        run.cache_root,
        home.join(".magicore").join("store").join("pypi")
    );
    let env = run.env_vars();
    let pip = env
        .iter()
        .find(|(k, _)| k == "PIP_CACHE_DIR")
        .map(|(_, v)| v.clone())
        .unwrap();
    let uv = env
        .iter()
        .find(|(k, _)| k == "UV_CACHE_DIR")
        .map(|(_, v)| v.clone())
        .unwrap();
    assert_eq!(pip, uv, "pip and uv must share ONE cache root");
    let (reused, _, mode) = run.finish();
    let _ = reused;
    assert_eq!(
        mode as i32,
        mgc_types::adapter::InstallCacheMode::MgCStore as i32
    );
}
