//! `dep_gate_canary.rs` — C0 ownership firewall runtime evidence (T0.3).
//!
//! Hermetic PATH canaries prove dependency lanes never spawn toolchains
//! in native mode and only spawn the opted-in toolchain under an
//! explicit compat opt-in (flag or `MGC_COMPAT_RUNTIME`), with the loud
//! warning every time. A refusal message on stderr proves nothing by
//! itself — the canary marker (or its silence) is the evidence.
//!
//! Kiểm toán runtime tường lửa C0: canary PATH hermetic chứng minh lane
//! dependency không spawn toolchain ở native, chỉ spawn tool đã opt-in
//! dưới compat tường minh (kèm cảnh báo lớn mỗi lần).

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

/// Fake toolchain canary: appends a marker line whenever resolved from
/// PATH, then exits 0 (so a compat-mode run can proceed through it).
struct CanarySandbox {
    #[allow(dead_code)]
    bin_dir: TempDir,
    home_dir: TempDir,
    canary_log: std::path::PathBuf,
}

impl CanarySandbox {
    fn new(tool: &str) -> Self {
        let bin_dir = TempDir::new().unwrap();
        let home_dir = TempDir::new().unwrap();
        let log_dir = bin_dir.path().join(".canary");
        std::fs::create_dir_all(&log_dir).unwrap();
        let log = log_dir.join("spawned.log");
        let sh = bin_dir.path().join(tool);
        std::fs::write(
            &sh,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"{} $*\" >> {}\nexit 0\n",
                tool,
                log.display()
            ),
        )
        .unwrap();
        make_executable(&sh);
        #[cfg(windows)]
        {
            let cmd = bin_dir.path().join(format!("{tool}.cmd"));
            std::fs::write(
                &cmd,
                format!(
                    "@echo off\necho {} %* >> {}\nexit /b 0\n",
                    tool,
                    log.display()
                ),
            )
            .unwrap();
        }
        Self {
            bin_dir,
            home_dir,
            canary_log: log,
        }
    }

    fn path_env(&self) -> std::ffi::OsString {
        let mut paths = vec![self.bin_dir.path().to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        std::env::join_paths(paths).unwrap()
    }

    fn marker_text(&self) -> String {
        std::fs::read_to_string(&self.canary_log).unwrap_or_default()
    }
}

fn make_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn run_mgc(
    args: &[&str],
    cwd: &std::path::Path,
    sandbox: &CanarySandbox,
    compat_env: Option<(&str, &str)>,
) -> (Option<i32>, String, String, String) {
    let mut cmd = Command::new(mgc_binary());
    cmd.args(args)
        .current_dir(cwd)
        .env("PATH", sandbox.path_env())
        // Hermetic HOME: shared-store env setup must not touch the real
        // home directory during tests.
        // (HOME hermetic: setup env store chung không được đụng home thật
        // trong test.)
        .env("HOME", sandbox.home_dir.path())
        .env_remove("MGC_COMPAT_RUNTIME");
    if let Some((key, value)) = compat_env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("spawn mgc");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        sandbox.marker_text(),
    )
}

fn ai_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"canary-ai\"\n[ai]\nframework = \"python-agent\"\n",
    )
    .unwrap();
    // uv.lock present → the lane deterministically picks `uv`.
    std::fs::write(dir.join("uv.lock"), "version = 1\n").unwrap();
}

fn flutter_project(dir: &std::path::Path) {
    std::fs::write(dir.join("pubspec.yaml"), "name: canary_app\n").unwrap();
}

fn rust_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"canary-lib\"\necosystem = \"lib\"\n[lib]\nlanguage = \"rust\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"canary-lib\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
}

#[test]
fn ai_install_native_refuses_without_spawning_uv() {
    let project = TempDir::new().unwrap();
    ai_project(project.path());
    let sandbox = CanarySandbox::new("uv");

    let (code, stdout, stderr, marker) = run_mgc(&["install-ai"], project.path(), &sandbox, None);
    let output = format!("{stdout}{stderr}");
    assert_ne!(
        code,
        Some(0),
        "native 'install-ai' must refuse the delegated lane:\n{output}"
    );
    assert!(
        marker.is_empty(),
        "uv canary must NEVER fire under native 'install-ai':\n{marker}"
    );
    assert!(
        output.contains("--compat-runtime"),
        "refusal must name the escape hatch:\n{output}"
    );
}

#[test]
fn ai_install_compat_spawns_uv_through_the_gate() {
    let project = TempDir::new().unwrap();
    ai_project(project.path());
    let sandbox = CanarySandbox::new("uv");

    let (code, stdout, stderr, marker) = run_mgc(
        &["install-ai"],
        project.path(),
        &sandbox,
        Some(("MGC_COMPAT_RUNTIME", "uv")),
    );
    let output = format!("{stdout}{stderr}");
    assert_eq!(code, Some(0), "compat install-ai must proceed:\n{output}");
    assert!(
        marker.contains("uv"),
        "compat must actually spawn uv through the gate:\n{}",
        sandbox.marker_text()
    );
    assert!(
        output.contains("COMPATIBILITY MODE"),
        "every compat spawn must warn loudly:\n{output}"
    );
}

#[test]
fn ai_add_native_refuses_without_spawning() {
    let project = TempDir::new().unwrap();
    ai_project(project.path());
    let sandbox = CanarySandbox::new("uv");

    let (code, stdout, stderr, marker) =
        run_mgc(&["add-ai", "requests"], project.path(), &sandbox, None);
    let output = format!("{stdout}{stderr}");
    assert_ne!(code, Some(0), "native 'add-ai' must refuse:\n{output}");
    assert!(
        marker.is_empty(),
        "uv canary must NEVER fire under native 'add-ai':\n{marker}"
    );
}

#[test]
fn ai_install_dry_run_needs_no_compat() {
    // Dry-run prints without spawning — the gate must not demand compat
    // for work that never happens.
    // (Dry-run chỉ in, không spawn — gate không được đòi compat.)
    let project = TempDir::new().unwrap();
    ai_project(project.path());
    let sandbox = CanarySandbox::new("uv");

    let (code, stdout, stderr, marker) =
        run_mgc(&["install-ai", "--dry-run"], project.path(), &sandbox, None);
    let output = format!("{stdout}{stderr}");
    assert_eq!(code, Some(0), "dry-run must succeed natively:\n{output}");
    assert!(marker.is_empty(), "dry-run must never spawn:\n{marker}");
}

#[test]
fn app_install_native_refuses_without_spawning_flutter() {
    let project = TempDir::new().unwrap();
    flutter_project(project.path());
    let sandbox = CanarySandbox::new("flutter");

    let (code, stdout, stderr, marker) = run_mgc(&["install-app"], project.path(), &sandbox, None);
    let output = format!("{stdout}{stderr}");
    assert_ne!(
        code,
        Some(0),
        "native 'install-app' must refuse the delegated lane:\n{output}"
    );
    assert!(
        marker.is_empty(),
        "flutter canary must NEVER fire under native 'install-app':\n{marker}"
    );
    assert!(
        output.contains("--compat-runtime"),
        "refusal must name the escape hatch:\n{output}"
    );
}

#[test]
fn app_install_compat_spawns_flutter_through_the_gate() {
    let project = TempDir::new().unwrap();
    flutter_project(project.path());
    let sandbox = CanarySandbox::new("flutter");

    let (code, stdout, stderr, marker) = run_mgc(
        &["install-app"],
        project.path(),
        &sandbox,
        Some(("MGC_COMPAT_RUNTIME", "flutter")),
    );
    let output = format!("{stdout}{stderr}");
    assert_eq!(code, Some(0), "compat install-app must proceed:\n{output}");
    assert!(
        marker.contains("flutter"),
        "compat must actually spawn flutter through the gate:\n{}",
        sandbox.marker_text()
    );
    assert!(
        output.contains("COMPATIBILITY MODE"),
        "every compat spawn must warn loudly:\n{output}"
    );
}
#[test]
fn lib_install_native_proceeds_without_spawning_cargo() {
    // Lib install is native (protocol resolve + verified fetch + CAS
    // materialize, zero toolchain spawns) — it must proceed natively AND
    // leave the cargo canary silent. The empty fixture has no dependencies,
    // so the native engine has nothing to fetch.
    let project = TempDir::new().unwrap();
    rust_lib_project(project.path());
    let sandbox = CanarySandbox::new("cargo");

    let (code, stdout, stderr, marker) = run_mgc(&["install-lib"], project.path(), &sandbox, None);
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native 'install-lib' (rust) is a native lane and must proceed:\n{output}"
    );
    assert!(
        marker.is_empty(),
        "cargo canary must NEVER fire under native 'install-lib':\n{marker}"
    );
}

#[test]
fn lib_add_native_refuses_without_spawning_cargo() {
    // Lib add delegates (cargo add) — native mode must refuse before any
    // spawn; compat opens the documented lane.
    let project = TempDir::new().unwrap();
    rust_lib_project(project.path());
    let sandbox = CanarySandbox::new("cargo");

    let (code, stdout, stderr, marker) =
        run_mgc(&["add-lib", "serde"], project.path(), &sandbox, None);
    let output = format!("{stdout}{stderr}");
    assert_ne!(
        code,
        Some(0),
        "native 'add-lib' (rust) must refuse the delegated lane:\n{output}"
    );
    assert!(
        marker.is_empty(),
        "cargo canary must NEVER fire under native 'add-lib':\n{marker}"
    );
    assert!(
        output.contains("--compat-runtime"),
        "refusal must name the escape hatch:\n{output}"
    );
}
