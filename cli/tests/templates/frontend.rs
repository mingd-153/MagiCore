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
fn test_real_bundler_build_succeeds() {
    // One fresh project proves native install, frozen replay, and a real framework build.
    // Một project mới kiểm chứng install native, frozen replay và build thật của framework.
    let name = format!("build-{FRAMEWORK}");
    let dir = common::scaffold(FRAMEWORK, &name);
    let (ok, out) = common::mgc_in(&dir, &["install-web"]);
    assert!(ok, "{FRAMEWORK} install (for build) failed:\n{out}");
    assert!(
        dir.join("node_modules").is_dir(),
        "{FRAMEWORK}: node_modules missing after install"
    );
    let lock = std::fs::read_to_string(dir.join("mgc.lock")).expect("mgc.lock written");
    assert!(
        lock.contains("version"),
        "{FRAMEWORK}: mgc.lock is not a lockfile"
    );
    let (ok, out) = common::mgc_in(&dir, &["install", "--frozen"]);
    assert!(ok, "{FRAMEWORK} frozen install on fresh lock failed:\n{out}");
    let (ok, out) = common::mgc_in(&dir, &["build"]);
    assert!(ok, "{FRAMEWORK} real bundler build failed:\n{out}");
}
