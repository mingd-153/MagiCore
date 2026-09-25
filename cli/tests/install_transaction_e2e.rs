//! `install_transaction_e2e.rs` — P0-1/P0-2 transaction contract, live:
//! a failing lifecycle script must leave the lockfile byte-identical and
//! the tree in its pre-lifecycle state (lock written only on success).
//! Failure is injected deterministically via MGC_LIFECYCLE_FAIL_PACKAGES
//! (no side effects — the test proves transaction boundaries, with a
//! REAL registry resolve + materialization underneath).
//! E2E hợp đồng transaction: script fail thì lock giữ nguyên byte và
//! cây về trạng thái pre-lifecycle (lock chỉ ghi khi thành công).

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn find_mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

/// Bounded registry reachability (skip without network, like the audit
/// lanes' osv_reachable guard).
/// (Không mạng thì skip, như guard osv_reachable.)
fn registry_reachable() -> bool {
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = "registry.npmjs.org:443".to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok()
}

fn fixture(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"txn-fixture\"\nversion = \"0.1.0\"\necosystem = \"web\"\nmode = \"frontend\"\nframeworks = []\ntemplate = \"\"\nfeatures = []\nregistries = []\npatches = []\n",
    )
    .unwrap();
    // esbuild ships a real postinstall (succeeds normally); the injected
    // failure forces the transaction path deterministically.
    // (esbuild có postinstall thật; failure tiêm ép đường transaction.)
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"txn-fixture","version":"1.0.0","dependencies":{"esbuild":"0.28.2"}}"#,
    )
    .unwrap();
}

fn run_install(
    mgc: &str,
    dir: &std::path::Path,
    fail_packages: Option<&str>,
) -> (Option<i32>, String) {
    let mut cmd = Command::new(mgc);
    cmd.args(["install", "--allow-scripts"]).current_dir(dir);
    if let Some(filter) = fail_packages {
        cmd.env("MGC_LIFECYCLE_FAIL_PACKAGES", filter);
    }
    let out = cmd.output().expect("failed to spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

/// P0-1: failing lifecycle + non-empty fetch graph → mgc.lock stays
/// byte-identical to the last successful install (no lock may certify a
/// failed state).
/// Script fail + fetch_graph khác rỗng → lock giữ nguyên từng byte.
#[test]
fn failing_lifecycle_keeps_lockfile_byte_identical() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let dir = std::env::temp_dir().join(format!("mgc-txn-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    fixture(&dir);

    // Baseline: successful install writes the lock.
    let (code, out) = run_install(&mgc, &dir, None);
    assert_eq!(code, Some(0), "baseline install must succeed:\n{out}");
    let lock_before = std::fs::read(dir.join("mgc.lock")).expect("lock from baseline");
    assert!(!lock_before.is_empty(), "baseline must write a lock");

    // Injected script failure: install errors AND the lock is untouched.
    // Delete node_modules first (keep mgc.lock!): otherwise the second
    // run short-circuits on the satisfied tree before lifecycle scripts
    // (npm parity — satisfied trees do not re-run dep scripts).
    // (Xóa node_modules, giữ lock: run-2 phải materialize + chạy script.)
    std::fs::remove_dir_all(dir.join("node_modules")).unwrap();
    let (code, out) = run_install(&mgc, &dir, Some("esbuild"));
    assert_ne!(code, Some(0), "injected script failure must fail install");
    assert!(
        out.contains("lifecycle script failed"),
        "error must name the lifecycle failure:\n{out}"
    );
    let lock_after = std::fs::read(dir.join("mgc.lock")).expect("lock must still exist");
    assert_eq!(
        lock_before, lock_after,
        "lockfile must be byte-identical after a failed install"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// P0-2: the materialized tree is restored to its pre-lifecycle state
/// (esbuild dir present with its manifest intact — not deleted, not
/// half-written).
/// Cây materialize về trạng thái pre-lifecycle (dir còn, manifest
/// nguyên — không xóa, không viết dở).
#[test]
fn failing_lifecycle_restores_pre_script_tree() {
    if !registry_reachable() {
        eprintln!("SKIP: registry unreachable");
        return;
    }
    let mgc = find_mgc_binary();
    let dir = std::env::temp_dir().join(format!("mgc-txn-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    fixture(&dir);

    let (code, out) = run_install(&mgc, &dir, None);
    assert_eq!(code, Some(0), "baseline install must succeed:\n{out}");
    let pkg_before = std::fs::read(
        dir.join("node_modules")
            .join("esbuild")
            .join("package.json"),
    )
    .expect("esbuild manifest from baseline");

    // Same re-materialization requirement as above: fresh tree, same lock.
    std::fs::remove_dir_all(dir.join("node_modules")).unwrap();
    let (code, _) = run_install(&mgc, &dir, Some("esbuild"));
    assert_ne!(code, Some(0), "injected script failure must fail install");
    let pkg_after = std::fs::read(
        dir.join("node_modules")
            .join("esbuild")
            .join("package.json"),
    )
    .expect("esbuild manifest must survive the rollback");
    assert_eq!(
        pkg_before, pkg_after,
        "pre-lifecycle tree must be restored byte-identical"
    );
    // No snapshot litter left behind in the project.
    // (Không còn rác snapshot trong project.)
    let litter: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".mgc-snap-"))
        .collect();
    assert!(
        litter.is_empty(),
        "snapshot litter must be gone: {litter:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
