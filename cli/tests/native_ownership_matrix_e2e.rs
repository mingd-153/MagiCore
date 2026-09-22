//! V1.2 Capability Registry matrix: EVERY native-owned (core, language,
//! operation) cell MUST execute with ZERO toolchain spawn. Delegated /
//! unsupported cells MUST fail closed (refuse + zero spawn) without
//! --compat-runtime. One table, executable proof — a lane labeled Native
//! that spawns fails this file (false-native canary).
//! (Ma trận registry V1.2: cell Native zero-spawn, cell khác fail-closed.)

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

const ALL_TOOLS: &[&str] = &[
    "cargo",
    "rustc",
    "pip",
    "pip3",
    "uv",
    "python",
    "python3",
    "go",
    "dotnet",
    "mvn",
    "java",
    "gradle",
    "flutter",
    "dart",
    "swift",
    "pod",
    "xcodebuild",
];

fn mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

struct MatrixSandbox {
    bin_dir: TempDir,
    home_dir: TempDir,
    canary_log: std::path::PathBuf,
}

impl MatrixSandbox {
    fn new() -> Self {
        let bin_dir = TempDir::new().unwrap();
        let home_dir = TempDir::new().unwrap();
        let log_dir = bin_dir.path().join(".canary");
        std::fs::create_dir_all(&log_dir).unwrap();
        let log = log_dir.join("spawned.log");
        for tool in ALL_TOOLS {
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&sh).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&sh, perms).unwrap();
            }
        }
        Self {
            bin_dir,
            home_dir,
            canary_log: log,
        }
    }

    fn run(&self, args: &[&str], cwd: &std::path::Path) -> (Option<i32>, String) {
        self.run_with_env(args, cwd, &[])
    }

    fn run_with_env(
        &self,
        args: &[&str],
        cwd: &std::path::Path,
        extra_env: &[(&str, &str)],
    ) -> (Option<i32>, String) {
        let mut cmd = Command::new(mgc_binary());
        cmd.args(args)
            .current_dir(cwd)
            .env("PATH", self.path_env())
            .env("HOME", self.home_dir.path())
            .env_remove("MGC_COMPAT_RUNTIME");
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("spawn mgc");
        (
            out.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }

    fn path_env(&self) -> std::ffi::OsString {
        let mut paths = vec![self.bin_dir.path().to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        std::env::join_paths(paths).unwrap()
    }

    fn assert_no_spawn(&self, what: &str) {
        let marker = std::fs::read_to_string(&self.canary_log).unwrap_or_default();
        assert!(
            marker.is_empty(),
            "{what}: ZERO toolchain spawn allowed, got:\n{marker}"
        );
    }
}

fn write(dir: &std::path::Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
}

fn lib_project(dir: &std::path::Path, language: &str, manifest_name: &str, manifest_body: &str) {
    write(
        dir,
        "mgc.toml",
        &format!("name = \"m\"\necosystem = \"lib\"\n[lib]\nlanguage = \"{language}\"\n"),
    );
    write(dir, manifest_name, manifest_body);
}

fn python_manifest() -> (&'static str, String) {
    (
        "pyproject.toml",
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = []\n"
            .to_string(),
    )
}

fn rust_manifest() -> (&'static str, String) {
    (
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n"
            .to_string(),
    )
}

fn go_manifest() -> (&'static str, String) {
    ("go.mod", "module m\n\ngo 1.21\n".to_string())
}

fn dotnet_manifest() -> (&'static str, String) {
    (
        "m.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n  </PropertyGroup>\n</Project>\n"
            .to_string(),
    )
}

fn java_manifest() -> (&'static str, String) {
    (
        "pom.xml",
        "<project>\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>com.example</groupId>\n  <artifactId>m</artifactId>\n  <version>0.1.0</version>\n</project>\n"
            .to_string(),
    )
}

fn ai_project(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"ai\"\n[ai]\nframework = \"python-agent\"\n",
    );
    let (_, body) = python_manifest();
    write(dir, "pyproject.toml", &body);
}

fn flutter_project(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"app\"\n[app]\nlanguage = \"flutter\"\n",
    );
    write(
        dir,
        "pubspec.yaml",
        "name: m\nenvironment:\n  sdk: \">=3.0.0 <4.0.0\"\ndependencies:\n  meta: ^1.12.0\n",
    );
}

/// Native add cell: succeeds + manifest pinned + zero spawn.
fn native_add_cell(
    setup: fn(&std::path::Path),
    add_cmd: &[&str],
    manifest_file: &str,
    pin: &str,
    lock_pin: &str,
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(add_cmd, project.path());
    assert_eq!(code, Some(0), "{} must succeed:\n{out}", add_cmd.join(" "));
    let body = std::fs::read_to_string(project.path().join(manifest_file)).unwrap();
    assert!(body.contains(pin), "manifest must pin {pin}:\n{body}");
    sandbox.assert_no_spawn(&add_cmd.join(" "));
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains(lock_pin),
        "mgc.lock must record {lock_pin}:\n{lock}"
    );
}

/// Native install cell (after add): succeeds + zero spawn.
fn native_install_cell(setup: fn(&std::path::Path), add_cmd: &[&str], install_cmd: &[&str]) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(add_cmd, project.path());
    assert_eq!(
        code,
        Some(0),
        "setup {} must succeed:\n{out}",
        add_cmd.join(" ")
    );
    let (code, out) = sandbox.run(install_cmd, project.path());
    assert_eq!(
        code,
        Some(0),
        "{} must succeed:\n{out}",
        install_cmd.join(" ")
    );
    sandbox.assert_no_spawn(&install_cmd.join(" "));
}

/// Default-blocked cell: refuses without compat + zero spawn.
fn blocked_cell(setup: fn(&std::path::Path), cmd: &[&str]) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(cmd, project.path());
    assert_ne!(
        code,
        Some(0),
        "{} must refuse without compat:\n{out}",
        cmd.join(" ")
    );
    sandbox.assert_no_spawn(&cmd.join(" "));
}

#[test]
fn matrix_lib_python_add_install_list_frozen() {
    let setup = |d: &std::path::Path| {
        let (f, b) = python_manifest();
        lib_project(d, "python", f, &b);
    };
    native_add_cell(setup, &["add-lib", "six"], "pyproject.toml", "six", "six");
    native_install_cell(setup, &["add-lib", "six"], &["install-lib"]);
    // List is a spawn-free manifest read.
    {
        let project = TempDir::new().unwrap();
        setup(project.path());
        let sandbox = MatrixSandbox::new();
        let (code, out) = sandbox.run(&["add-lib", "six"], project.path());
        assert_eq!(code, Some(0), "setup add must succeed:\n{out}");
        let (code, out) = sandbox.run(&["list-lib"], project.path());
        assert_eq!(code, Some(0), "list-lib must succeed:\n{out}");
        assert!(out.contains("six"), "list must show six:\n{out}");
        sandbox.assert_no_spawn("list-lib");
    }
    // Frozen replays the lock with zero spawn.
    {
        let project = TempDir::new().unwrap();
        setup(project.path());
        let sandbox = MatrixSandbox::new();
        let (code, out) = sandbox.run(&["add-lib", "six"], project.path());
        assert_eq!(code, Some(0), "setup add must succeed:\n{out}");
        let (code, out) = sandbox.run(&["install", "--frozen"], project.path());
        assert_eq!(code, Some(0), "frozen install must succeed:\n{out}");
        sandbox.assert_no_spawn("install --frozen");
    }
}

#[test]
fn matrix_lib_rust_add_install() {
    let setup = |d: &std::path::Path| {
        let (f, b) = rust_manifest();
        lib_project(d, "rust", f, &b);
    };
    native_add_cell(
        setup,
        &["add-lib", "serde_json"],
        "Cargo.toml",
        "serde_json",
        "serde_json",
    );
    native_install_cell(setup, &["add-lib", "serde_json"], &["install-lib"]);
}

#[test]
fn matrix_lib_go_add_install() {
    let setup = |d: &std::path::Path| {
        let (f, b) = go_manifest();
        lib_project(d, "go", f, &b);
    };
    native_add_cell(
        setup,
        &["add-lib", "github.com/google/uuid"],
        "go.mod",
        "github.com/google/uuid",
        "github.com/google/uuid",
    );
    native_install_cell(
        setup,
        &["add-lib", "github.com/google/uuid"],
        &["install-lib"],
    );
}

#[test]
fn matrix_lib_dotnet_add_install() {
    let setup = |d: &std::path::Path| {
        let (f, b) = dotnet_manifest();
        lib_project(d, "dotnet", f, &b);
    };
    native_add_cell(
        setup,
        &["add-lib", "Newtonsoft.Json"],
        "m.csproj",
        "Newtonsoft.Json",
        "Newtonsoft.Json",
    );
    native_install_cell(setup, &["add-lib", "Newtonsoft.Json"], &["install-lib"]);
}

#[test]
fn matrix_lib_java_add_install() {
    let setup = |d: &std::path::Path| {
        let (f, b) = java_manifest();
        lib_project(d, "java", f, &b);
    };
    native_add_cell(
        setup,
        &["add-lib", "org.apache.commons:commons-lang3"],
        "pom.xml",
        "commons-lang3",
        "commons-lang3",
    );
    native_install_cell(
        setup,
        &["add-lib", "org.apache.commons:commons-lang3"],
        &["install-lib"],
    );
}

#[test]
fn matrix_ai_python_add_install() {
    native_add_cell(
        ai_project,
        &["add-ai", "six"],
        "pyproject.toml",
        "six",
        "six",
    );
    native_install_cell(ai_project, &["add-ai", "six"], &["install-ai"]);
}

#[test]
fn matrix_app_flutter_install() {
    let project = TempDir::new().unwrap();
    flutter_project(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["install-app"], project.path());
    assert_eq!(code, Some(0), "install-app must succeed:\n{out}");
    sandbox.assert_no_spawn("install-app");
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(lock.contains("meta"), "mgc.lock must record meta:\n{lock}");
}

/// Native remove cell: add, then remove — the manifest must drop the
/// pin (no stale entry left behind) with zero spawn.
fn native_remove_cell(
    setup: fn(&std::path::Path),
    add_cmd: &[&str],
    remove_cmd: &[&str],
    manifest_file: &str,
    pin: &str,
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(add_cmd, project.path());
    assert_eq!(
        code,
        Some(0),
        "setup {} must succeed:\n{out}",
        add_cmd.join(" ")
    );
    let (code, out) = sandbox.run(remove_cmd, project.path());
    assert_eq!(
        code,
        Some(0),
        "{} must succeed:\n{out}",
        remove_cmd.join(" ")
    );
    let body = std::fs::read_to_string(project.path().join(manifest_file)).unwrap();
    assert!(!body.contains(pin), "manifest must drop {pin}:\n{body}");
    sandbox.assert_no_spawn(&remove_cmd.join(" "));
}

#[test]
fn matrix_native_remove_all_lanes() {
    let py = |d: &std::path::Path| {
        let (f, b) = python_manifest();
        lib_project(d, "python", f, &b);
    };
    native_remove_cell(
        py,
        &["add-lib", "six"],
        &["remove-lib", "six"],
        "pyproject.toml",
        "six",
    );
    let rs = |d: &std::path::Path| {
        let (f, b) = rust_manifest();
        lib_project(d, "rust", f, &b);
    };
    native_remove_cell(
        rs,
        &["add-lib", "serde_json"],
        &["remove-lib", "serde_json"],
        "Cargo.toml",
        "serde_json",
    );
    let go = |d: &std::path::Path| {
        let (f, b) = go_manifest();
        lib_project(d, "go", f, &b);
    };
    native_remove_cell(
        go,
        &["add-lib", "github.com/google/uuid"],
        &["remove-lib", "github.com/google/uuid"],
        "go.mod",
        "github.com/google/uuid",
    );
    let net = |d: &std::path::Path| {
        let (f, b) = dotnet_manifest();
        lib_project(d, "dotnet", f, &b);
    };
    native_remove_cell(
        net,
        &["add-lib", "Newtonsoft.Json"],
        &["remove-lib", "Newtonsoft.Json"],
        "m.csproj",
        "Newtonsoft.Json",
    );
    let java = |d: &std::path::Path| {
        let (f, b) = java_manifest();
        lib_project(d, "java", f, &b);
    };
    native_remove_cell(
        java,
        &["add-lib", "org.apache.commons:commons-lang3"],
        &["remove-lib", "org.apache.commons:commons-lang3"],
        "pom.xml",
        "commons-lang3",
    );
}

#[test]
fn matrix_default_blocked_cells_refuse_silently_spawn_free() {
    // ai Update stays delegated — without compat it must refuse AND
    // spawn nothing. (All lib verbs are native now.)
    let ai = |d: &std::path::Path| {
        write(
            d,
            "mgc.toml",
            "name = \"m\"\necosystem = \"ai\"\n[ai]\nframework = \"python-agent\"\n",
        );
        let (_, b) = python_manifest();
        write(d, "pyproject.toml", &b);
    };
    blocked_cell(ai, &["update-ai", "six"]);
}

#[test]
fn matrix_offline_reinstall_serves_from_cache_no_registry() {
    // Warm install, then kill the registry: lock short-circuit + CAS
    // must serve with zero network and zero spawn.
    let project = TempDir::new().unwrap();
    let (f, b) = python_manifest();
    lib_project(project.path(), "python", f, &b);
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["add-lib", "six"], project.path());
    assert_eq!(code, Some(0), "warm add must succeed:\n{out}");
    let (code, out) = sandbox.run(&["install-lib"], project.path());
    assert_eq!(code, Some(0), "warm install must succeed:\n{out}");
    let (code, out) = sandbox.run_with_env(
        &["install-lib"],
        project.path(),
        &[("MGC_PYPI_INDEX_URL", "http://127.0.0.1:1")],
    );
    assert_eq!(
        code,
        Some(0),
        "offline reinstall (dead registry) must succeed from lock+CAS:\n{out}"
    );
    sandbox.assert_no_spawn("offline install-lib");
}

#[test]
fn matrix_registry_outage_fails_closed_no_hang() {
    // Dead registry + cold cache: honest network error, never a hang,
    // never a false success, never a spawn.
    let project = TempDir::new().unwrap();
    let (f, b) = python_manifest();
    lib_project(project.path(), "python", f, &b);
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(
        &["add-lib", "six"],
        project.path(),
        &[("MGC_PYPI_INDEX_URL", "http://127.0.0.1:1")],
    );
    assert_ne!(
        code,
        Some(0),
        "cold add against a dead registry must fail:\n{out}"
    );
    assert!(
        out.to_lowercase().contains("network")
            || out.to_lowercase().contains("connect")
            || out.to_lowercase().contains("refused"),
        "failure must name the network cause:\n{out}"
    );
    sandbox.assert_no_spawn("outage add-lib");
}

#[test]
fn matrix_tampered_lock_fails_closed() {
    // Downgraded pin (out of manifest range): frozen must refuse, never
    // silently re-resolve.
    let project = TempDir::new().unwrap();
    let (f, b) = python_manifest();
    lib_project(project.path(), "python", f, &b);
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["add-lib", "six"], project.path());
    assert_eq!(code, Some(0), "setup add must succeed:\n{out}");
    let lock_path = project.path().join("mgc.lock");
    let tampered = std::fs::read_to_string(&lock_path)
        .unwrap()
        .replacen("1.17.0", "1.0.0", 1);
    assert_ne!(
        std::fs::read_to_string(&lock_path).unwrap(),
        tampered,
        "fixture must actually change"
    );
    std::fs::write(&lock_path, tampered).unwrap();
    let (code, out) = sandbox.run(&["install", "--frozen"], project.path());
    assert_ne!(
        code,
        Some(0),
        "frozen install on a tampered lock must fail:\n{out}"
    );
    sandbox.assert_no_spawn("tampered frozen install");
}

#[test]
fn matrix_tampered_integrity_fails_verification() {
    // Corrupted integrity hash: verified fetch must reject, exit != 0.
    let project = TempDir::new().unwrap();
    let (f, b) = python_manifest();
    lib_project(project.path(), "python", f, &b);
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["add-lib", "six"], project.path());
    assert_eq!(code, Some(0), "setup add must succeed:\n{out}");
    let lock_path = project.path().join("mgc.lock");
    let body = std::fs::read_to_string(&lock_path).unwrap();
    let start = body.find("sha256-").expect("lock carries integrity");
    let mut tampered = body.clone();
    tampered.replace_range(start + 7..start + 8, "A");
    assert_ne!(body, tampered, "fixture must actually change");
    std::fs::write(&lock_path, tampered).unwrap();
    let (code, out) = sandbox.run(&["install-lib"], project.path());
    assert_ne!(
        code,
        Some(0),
        "install on corrupted integrity must fail:\n{out}"
    );
    assert!(
        out.to_lowercase().contains("integrity") || out.to_lowercase().contains("mismatch"),
        "failure must name integrity:\n{out}"
    );
    sandbox.assert_no_spawn("tampered install-lib");
}

/// Native update cell: outdated pin bumps to latest with zero spawn.
fn native_update_cell(
    setup: fn(&std::path::Path),
    update_cmd: &[&str],
    manifest_file: &str,
    old_pin: &str,
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(update_cmd, project.path());
    assert_eq!(
        code,
        Some(0),
        "{} must succeed:\n{out}",
        update_cmd.join(" ")
    );
    let body = std::fs::read_to_string(project.path().join(manifest_file)).unwrap();
    assert!(
        !body.contains(old_pin),
        "manifest must drop outdated pin {old_pin}:\n{body}"
    );
    sandbox.assert_no_spawn(&update_cmd.join(" "));
}

fn outdated_python(dir: &std::path::Path) {
    lib_project(
        dir,
        "python",
        "pyproject.toml",
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = [\"six==1.15.0\"]\n",
    );
}

fn outdated_rust(dir: &std::path::Path) {
    lib_project(
        dir,
        "rust",
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde_json = \"1.0.100\"\n",
    );
}

fn outdated_go(dir: &std::path::Path) {
    lib_project(
        dir,
        "go",
        "go.mod",
        "module m\n\ngo 1.21\n\nrequire github.com/google/uuid v1.0.0\n",
    );
}

fn outdated_dotnet(dir: &std::path::Path) {
    lib_project(
        dir,
        "dotnet",
        "m.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n  </PropertyGroup>\n  <ItemGroup>\n    <PackageReference Include=\"Newtonsoft.Json\" Version=\"13.0.1\" />\n  </ItemGroup>\n</Project>\n",
    );
}

fn outdated_java(dir: &std::path::Path) {
    lib_project(
        dir,
        "java",
        "pom.xml",
        "<project>\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>com.example</groupId>\n  <artifactId>m</artifactId>\n  <version>0.1.0</version>\n  <dependencies>\n    <dependency>\n      <groupId>org.apache.commons</groupId>\n      <artifactId>commons-lang3</artifactId>\n      <version>3.12.0</version>\n    </dependency>\n  </dependencies>\n</project>\n",
    );
}

#[test]
fn matrix_native_update_python_rust() {
    native_update_cell(
        outdated_python,
        &["update-lib", "six"],
        "pyproject.toml",
        "1.15.0",
    );
    native_update_cell(
        outdated_rust,
        &["update-lib", "serde_json"],
        "Cargo.toml",
        "1.0.100",
    );
}

#[test]
fn matrix_native_update_go_dotnet_java() {
    native_update_cell(
        outdated_go,
        &["update-lib", "github.com/google/uuid"],
        "go.mod",
        "v1.0.0",
    );
    native_update_cell(
        outdated_dotnet,
        &["update-lib", "Newtonsoft.Json"],
        "m.csproj",
        "13.0.1",
    );
    native_update_cell(
        outdated_java,
        &["update-lib", "org.apache.commons:commons-lang3"],
        "pom.xml",
        "3.12.0",
    );
}

fn outdated_flutter(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"app\"\n[app]\nlanguage = \"flutter\"\n",
    );
    write(
        dir,
        "pubspec.yaml",
        "name: m\nenvironment:\n  sdk: \">=3.0.0 <4.0.0\"\ndependencies:\n  meta: 1.9.1\n",
    );
}

fn outdated_ai(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"ai\"\n[ai]\nframework = \"python-agent\"\n",
    );
    write(
        dir,
        "pyproject.toml",
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = [\"six==1.15.0\"]\n",
    );
}

#[test]
fn matrix_native_update_flutter_ai() {
    native_update_cell(
        outdated_flutter,
        &["update-app", "meta"],
        "pubspec.yaml",
        "1.9.1",
    );
    native_update_cell(
        outdated_ai,
        &["update-ai", "six"],
        "pyproject.toml",
        "1.15.0",
    );
}

fn bevy_project(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"game\"\n[game]\nengine = \"bevy\"\n",
    );
    write(
        dir,
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    );
}

fn esp32_project(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"iot\"\n[iot]\nframework = \"esp32-rust\"\n",
    );
    write(
        dir,
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
}

#[test]
fn matrix_game_bevy_add_install_native() {
    let project = TempDir::new().unwrap();
    bevy_project(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["add-game", "serde_json"], project.path());
    assert_eq!(code, Some(0), "native bevy `add-game` must succeed:\n{out}");
    let body = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(
        body.contains("serde_json"),
        "Cargo.toml must pin serde_json:\n{body}"
    );
    sandbox.assert_no_spawn("add-game");
    let (code, out) = sandbox.run(&["install-game"], project.path());
    assert_eq!(
        code,
        Some(0),
        "native bevy `install-game` must succeed:\n{out}"
    );
    sandbox.assert_no_spawn("install-game");
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("serde_json"),
        "mgc.lock must record serde_json:\n{lock}"
    );
}

#[test]
fn matrix_iot_esp32_add_install_native() {
    let project = TempDir::new().unwrap();
    esp32_project(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["add-iot", "serde_json"], project.path());
    assert_eq!(code, Some(0), "native esp32 `add-iot` must succeed:\n{out}");
    let body = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(
        body.contains("serde_json"),
        "Cargo.toml must pin serde_json:\n{body}"
    );
    sandbox.assert_no_spawn("add-iot");
    let (code, out) = sandbox.run(&["install-iot"], project.path());
    assert_eq!(
        code,
        Some(0),
        "native esp32 `install-iot` must succeed:\n{out}"
    );
    sandbox.assert_no_spawn("install-iot");
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("serde_json"),
        "mgc.lock must record serde_json:\n{lock}"
    );
}
