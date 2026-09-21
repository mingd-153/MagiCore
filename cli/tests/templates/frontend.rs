// Shared frontend-framework E2E: scaffold → NATIVE install → real
// bundler build, through the same web/npm engine every JS framework
// rides (no per-framework pipeline — parity by construction).
// Including file MUST define `const FRAMEWORK: &str` before include.
// Template E2E chung cho framework frontend: scaffold → install native
// → build bundler thật, cùng engine web/npm.

#[test]
fn test_scaffold_succeeds() {
    let name = format!("test-{FRAMEWORK}");
    let dir = common::scaffold(FRAMEWORK, &name);
    assert!(dir.exists(), "project dir {name} created");
    common::assert_file_exists(&dir, "package.json");
    common::assert_file_exists(&dir, ".gitignore");
}

#[test]
fn test_native_install_materializes_deps_and_lock() {
    // REAL resolve/lock/fetch/CAS/materialize via the native web
    // engine (not scaffold-only): node_modules + valid mgc.lock prove it.
    // (Install thật qua engine native: node_modules + lock hợp lệ.)
    let name = format!("install-{FRAMEWORK}");
    let dir = common::scaffold(FRAMEWORK, &name);
    let (ok, out) = common::mgc_in(&dir, &["install-web"]);
    assert!(ok, "{FRAMEWORK} native install failed:\n{out}");
    assert!(
        dir.join("node_modules").is_dir(),
        "{FRAMEWORK}: node_modules missing after install"
    );
    let lock = std::fs::read_to_string(dir.join("mgc.lock")).expect("mgc.lock written");
    assert!(
        lock.contains("version"),
        "{FRAMEWORK}: mgc.lock is not a lockfile"
    );
    // A valid lock must satisfy a frozen install (real proof the lock is
    // usable, not just present).
    // (Lock hợp lệ phải qua frozen install.)
    let (ok, out) = common::mgc_in(&dir, &["install", "--frozen"]);
    assert!(ok, "{FRAMEWORK} frozen install on fresh lock failed:\n{out}");
}

#[test]
fn test_real_bundler_build_succeeds() {
    // The framework's OWN build script (vite/ng/next/astro...) runs to
    // exit 0 — a real bundle, not a scaffold listing.
    // (Build script của chính framework chạy exit 0 — bundle thật.)
    let name = format!("build-{FRAMEWORK}");
    let dir = common::scaffold(FRAMEWORK, &name);
    let (ok, _) = common::mgc_in(&dir, &["install-web"]);
    assert!(ok, "{FRAMEWORK} install (for build) failed");
    let (ok, out) = common::mgc_in(&dir, &["build"]);
    assert!(ok, "{FRAMEWORK} real bundler build failed:\n{out}");
}
