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
    // Full executable universe (P0 canary gap): every binary a dependency
    // operation could conceivably spawn must be spied — an unlisted tool
    // spawning silently is a false-native hole.
    // (Mọi binary operation có thể spawn đều bị theo dõi.)
    "node",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "bun",
    "bunx",
    "deno",
    "composer",
    "git",
    "terraform",
    "pio",
    "platformio",
    "west",
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

/// A default-blocked mutation must preserve its manifest byte-for-byte.
/// (Mutation bị chặn mặc định phải giữ manifest nguyên từng byte.)
fn blocked_mutation_unchanged(setup: fn(&std::path::Path), cmd: &[&str], manifest_file: &str) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let manifest = project.path().join(manifest_file);
    let before = std::fs::read(&manifest).unwrap();
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(cmd, project.path());
    assert_ne!(
        code,
        Some(0),
        "{} must refuse without compat:\n{out}",
        cmd.join(" ")
    );
    assert_eq!(
        std::fs::read(&manifest).unwrap(),
        before,
        "blocked command mutated {manifest_file}"
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
fn matrix_ai_python_native_list_remove_use_verified_adapter_without_tools() {
    let project = TempDir::new().unwrap();
    ai_project(project.path());
    let sandbox = MatrixSandbox::new();

    let (code, out) = sandbox.run(&["list-ai"], project.path());
    assert_eq!(code, Some(0), "native list-ai must proceed:\n{out}");
    sandbox.assert_no_spawn("list-ai (native ai/python)");

    let (code, out) = sandbox.run(&["remove-ai", "not-present"], project.path());
    assert_eq!(code, Some(0), "native remove-ai no-op must proceed:\n{out}");
    sandbox.assert_no_spawn("remove-ai (native ai/python)");
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

/// Native remove on a fixture that already carries the dep.
fn native_remove_only(
    setup: fn(&std::path::Path),
    remove_cmd: &[&str],
    manifest_file: &str,
    pin: &str,
) {
    native_remove_only_with_env(setup, remove_cmd, manifest_file, pin, &[]);
}

fn native_remove_only_with_env(
    setup: fn(&std::path::Path),
    remove_cmd: &[&str],
    manifest_file: &str,
    pin: &str,
    extra_env: &[(&str, &str)],
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let body_before = std::fs::read_to_string(project.path().join(manifest_file)).unwrap();
    assert!(
        body_before.contains(pin),
        "fixture must carry {pin}:
{body_before}"
    );
    let (code, out) = sandbox.run_with_env(remove_cmd, project.path(), extra_env);
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
fn matrix_cloud_dependency_commands_fail_closed_without_native_manifest() {
    for (framework, commands) in [
        (
            "cdk",
            vec![
                vec!["install-clo"],
                vec!["add-clo", "constructs"],
                vec!["remove-clo", "constructs"],
                vec!["update-clo", "constructs"],
            ],
        ),
        (
            "terraform",
            vec![
                vec!["install-clo"],
                vec!["add-clo", "random"],
                vec!["remove-clo", "random"],
                vec!["update-clo", "random"],
            ],
        ),
    ] {
        for command in commands {
            let project = TempDir::new().unwrap();
            write(
                project.path(),
                "mgc.toml",
                &format!("name = \"m\"\necosystem = \"cloud\"\n[cloud]\ntype = \"{framework}\"\n"),
            );
            if framework == "terraform" {
                write(project.path(), "main.tf", "terraform {}\n");
            }
            // A CDK/Pulumi project without package.json is not an admitted
            // native JS lane; Terraform is never a package dependency lane.
            let sandbox = MatrixSandbox::new();
            let (code, out) = sandbox.run(&command, project.path());
            assert_ne!(
                code,
                Some(0),
                "clo/{framework} {} without package.json must refuse:\n{out}",
                command.join(" ")
            );
            assert!(
                out.contains("unsupported")
                    || out.contains("not supported")
                    || out.contains("native"),
                "refusal should explain missing native ownership:\n{out}"
            );
            sandbox.assert_no_spawn(&command.join(" "));
        }
    }
}

#[test]
fn matrix_capabilities_binary_emits_closed_owner_for_every_framework_operation() {
    let cwd = TempDir::new().unwrap();
    let sandbox = MatrixSandbox::new();
    let (code, output) = sandbox.run(&["capabilities"], cwd.path());
    assert_eq!(code, Some(0), "mgc capabilities must succeed:\n{output}");
    sandbox.assert_no_spawn("mgc capabilities");

    let document: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|error| panic!("capabilities must emit valid JSON: {error}\n{output}"));
    let cores = document["cores"]
        .as_array()
        .expect("all-core capabilities must be an array");
    let required_cores = [
        "ai", "app", "cicd", "clo", "game", "hardware", "iot", "lib", "web",
    ];
    for core in required_cores {
        let row = cores
            .iter()
            .find(|row| row["core"] == core)
            .unwrap_or_else(|| panic!("mgc capabilities omitted core {core}"));
        let frameworks = row["framework_qualification"]
            .as_array()
            .unwrap_or_else(|| panic!("{core} framework qualification must be an array"));
        for framework in frameworks {
            let framework_name = framework["framework"].as_str().unwrap_or("<missing>");
            let ownership = framework["dependency_ownership"]
                .as_object()
                .unwrap_or_else(|| panic!("{core}/{framework_name} has no operation ownership"));
            for operation in [
                "install",
                "add",
                "remove",
                "update",
                "list",
                "resolve",
                "lock",
                "fetch",
                "verify",
                "store",
                "materialize",
                "frozen-install",
                "offline-reinstall",
                "gc",
            ] {
                let cell = ownership.get(operation).unwrap_or_else(|| {
                    panic!("{core}/{framework_name} missing ownership cell {operation}")
                });
                assert!(
                    matches!(
                        cell["owner"].as_str(),
                        Some("mgc-native" | "scaffold-only" | "unsupported")
                    ),
                    "{core}/{framework_name}/{operation} has invalid owner: {cell}"
                );
            }
        }
    }
    let cloud = cores.iter().find(|row| row["core"] == "clo").unwrap();
    let cdk = cloud["framework_qualification"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["framework"] == "cdk")
        .unwrap();
    assert_eq!(
        cdk["dependency_ownership"]["install"]["requires"],
        "package.json; embedded MGC JavaScript engine"
    );
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

fn outdated_bevy(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"game\"\n[game]\nengine = \"bevy\"\n",
    );
    write(
        dir,
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde_json = \"1.0.100\"\n",
    );
}

fn outdated_esp32(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"iot\"\n[iot]\nframework = \"esp32-rust\"\n",
    );
    write(
        dir,
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde_json = \"1.0.100\"\n",
    );
}

#[test]
fn matrix_native_update_game_iot() {
    native_update_cell(
        outdated_bevy,
        &["update-game", "serde_json"],
        "Cargo.toml",
        "1.0.100",
    );
    native_update_cell(
        outdated_esp32,
        &["update-iot", "serde_json"],
        "Cargo.toml",
        "1.0.100",
    );
}

fn flutter_with_meta(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"app\"\n[app]\nlanguage = \"flutter\"\n",
    );
    write(
        dir,
        "pubspec.yaml",
        "name: m\nenvironment:\n  sdk: \">=3.0.0 <4.0.0\"\ndependencies:\n  flutter:\n    sdk: flutter\n  meta: ^1.12.0\n",
    );
}

fn swift_with_dep(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"app\"\n[app]\nlanguage = \"swift\"\n",
    );
    write(
        dir,
        "Package.swift",
        concat!(
            "// swift-tools-version: 5.9\n",
            "import PackageDescription\n\n",
            "let package = Package(\n",
            "    name: \"m\",\n",
            "    dependencies: [\n",
            "        .package(id: \"scope.lib\", from: \"1.0.0\"),\n",
            "    ],\n",
            ")\n",
        ),
    );
}

fn kotlin_with_dep(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"app\"\n[app]\nlanguage = \"kotlin\"\n",
    );
    write(dir, "settings.gradle.kts", "rootProject.name = \"m\"\n");
    std::fs::create_dir_all(dir.join("gradle")).unwrap();
    std::fs::create_dir_all(dir.join("gradle")).unwrap();
    write(
        dir,
        "gradle/libs.versions.toml",
        "[versions]\nlang3 = \"3.14.0\"\n\n[libraries]\ncommons-lang3 = { module = \"org.apache.commons:commons-lang3\", version.ref = \"lang3\" }\n",
    );
    write(
        dir,
        "build.gradle.kts",
        "plugins { kotlin(\"jvm\") version \"2.0.0\" }\ndependencies {\n    implementation(libs.commons.lang3)\n}\n",
    );
}

fn bevy_with_dep(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"game\"\n[game]\nengine = \"bevy\"\n",
    );
    write(
        dir,
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde_json = \"1.0.100\"\n",
    );
}

fn esp32_with_dep(dir: &std::path::Path) {
    write(
        dir,
        "mgc.toml",
        "name = \"m\"\necosystem = \"iot\"\n[iot]\nframework = \"esp32-rust\"\n",
    );
    write(
        dir,
        "Cargo.toml",
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde_json = \"1.0.100\"\n",
    );
}

#[test]
fn matrix_native_remove_flutter_swift_kotlin() {
    // Flutter's journaled writer owns remove. Swift/Kotlin are blocked at
    // the C0 firewall until their writers join that transaction; never
    // label a refusal as native remove support.
    // (Flutter dùng writer có journal; Swift/Kotlin bị chặn tới khi có transaction.)
    let sdk = TempDir::new().unwrap();
    let flutter_package = sdk.path().join("packages/flutter");
    std::fs::create_dir_all(flutter_package.join("lib")).unwrap();
    write(&flutter_package, "pubspec.yaml", "name: flutter\n");
    let sdk_root = sdk.path().to_string_lossy().to_string();
    native_remove_only_with_env(
        flutter_with_meta,
        &["remove-app", "meta"],
        "pubspec.yaml",
        "meta",
        &[("FLUTTER_ROOT", &sdk_root)],
    );
    blocked_mutation_unchanged(
        swift_with_dep,
        &["remove-app", "scope/lib"],
        "Package.swift",
    );
    blocked_mutation_unchanged(
        kotlin_with_dep,
        &["remove-app", "commons-lang3"],
        "gradle/libs.versions.toml",
    );
}

#[test]
fn matrix_native_remove_game_iot() {
    native_remove_only(
        bevy_with_dep,
        &["remove-game", "serde_json"],
        "Cargo.toml",
        "serde_json",
    );
    native_remove_only(
        esp32_with_dep,
        &["remove-iot", "serde_json"],
        "Cargo.toml",
        "serde_json",
    );
}

#[test]
fn matrix_remove_rolls_back_manifest_when_install_fails() {
    // Atomic remove: manifest edited, then install tail hits a dead
    // registry → the op fails AND the manifest is restored (never a
    // half-state where the dep is dropped but lock/store still carry it).
    let project = TempDir::new().unwrap();
    let (f, b) = python_manifest();
    // Fixture carries TWO deps so remove has work and rollback is visible.
    lib_project(
        project.path(),
        "python",
        f,
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = [\"six==1.17.0\", \"attrs==23.1.0\"]\n",
    );
    let _ = b;
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["install-lib"], project.path());
    assert_eq!(code, Some(0), "warm install must succeed:\n{out}");
    let before = std::fs::read_to_string(project.path().join("pyproject.toml")).unwrap();
    assert!(
        before.contains("attrs"),
        "fixture must carry attrs:\n{before}"
    );
    // Break the artifact fetch (dead host in the lock URL) with a cold
    // store: the install tail must fail at fetch, triggering rollback.
    // (Lock short-circuit skips resolve, so killing the index alone is
    // not enough — the fetch itself must fail.)
    let lock_path = project.path().join("mgc.lock");
    let lock_body = std::fs::read_to_string(&lock_path).unwrap();
    let broken = lock_body.replacen("https://files.pythonhosted.org", "http://127.0.0.1:1", 1);
    assert_ne!(lock_body, broken, "fixture must actually break");
    std::fs::write(&lock_path, broken).unwrap();
    // Re-snapshot AFTER tampering: the rollback must restore exactly
    // this (tampered) lock byte-identical — the tail must not rewrite it
    // on its way to failing.
    // (Chụp lại lock SAU khi phá — rollback phải trả đúng từng byte.)
    let lock_bytes_before = std::fs::read(&lock_path).unwrap();
    let cold = MatrixSandbox::new();
    let (code, out) = cold.run(&["remove-lib", "attrs"], project.path());
    assert_ne!(
        code,
        Some(0),
        "remove whose install tail hits a dead registry must fail:\n{out}"
    );
    let after = std::fs::read_to_string(project.path().join("pyproject.toml")).unwrap();
    assert!(
        after.contains("attrs"),
        "rolled-back manifest must still carry attrs:\n{after}"
    );
    let lock_bytes_after = std::fs::read(project.path().join("mgc.lock")).unwrap();
    assert_eq!(
        lock_bytes_after, lock_bytes_before,
        "rolled-back lock must be byte-identical (tail must not rewrite it on failure)"
    );
    cold.assert_no_spawn("atomic remove-lib");
}

/// Hermetic PyPI registry (mockito): serves JSON metadata + wheels for
/// fake distributions that provably do NOT exist on the real PyPI — a
/// passing test proves every byte came from the fixture, never the
/// network. (Registry PyPI hermetic — package giả, không mạng thật.)
struct HermeticPypi {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    url: String,
}

/// Build a minimal pure-python wheel with a CORRECT RECORD (sha256
/// base64url-nopad + sizes, RECORD row self-empty) so the PEP 376
/// verifier accepts it exactly like a registry wheel.
/// (Dựng wheel tối thiểu với RECORD đúng chuẩn.)
fn build_test_wheel(dist: &str, version: &str) -> Vec<u8> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let dist_info = format!("{dist}-{version}.dist-info");
    let pkg_dir = dist.to_string();
    let files: Vec<(String, Vec<u8>)> = vec![
        (
            format!("{pkg_dir}/__init__.py"),
            b"VALUE = 1\n".to_vec(),
        ),
        (
            format!("{dist_info}/METADATA"),
            format!("Metadata-Version: 2.1\nName: {}\nVersion: {version}\n", dist.replace('_', "-"))
                .into_bytes(),
        ),
        (
            format!("{dist_info}/WHEEL"),
            b"Wheel-Version: 1.0\nGenerator: mgc-hermetic-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
                .to_vec(),
        ),
    ];
    let mut record = String::new();
    for (rel, data) in &files {
        let mut hasher = Sha256::new();
        hasher.update(data);
        record.push_str(&format!(
            "{rel},sha256={},{}\n",
            b64.encode(hasher.finalize()),
            data.len()
        ));
    }
    record.push_str(&format!("{dist_info}/RECORD,,\n"));
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (rel, data) in &files {
        writer.start_file(rel, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer
        .start_file(format!("{dist_info}/RECORD"), options)
        .unwrap();
    writer.write_all(record.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

impl HermeticPypi {
    /// Serve the given (dist, version) fake distributions with valid
    /// wheels + metadata (project + per-version JSON, like the real API).
    fn new(server_pkgs: &[(&str, &str)]) -> Self {
        let mut server = mockito::Server::new();
        let url = server.url();
        let mut mocks = Vec::new();
        for (dist, version) in server_pkgs {
            let wheel = build_test_wheel(dist, version);
            let filename = format!("{dist}-{version}-py3-none-any.whl");
            let wheel_url = format!("{url}/files/{filename}");
            let hex = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&wheel);
                format!("{:x}", hasher.finalize())
            };
            let doc = serde_json::json!({
                "info": { "requires_python": ">=3.8" },
                "releases": { version.to_string(): [{
                    "filename": filename,
                    "url": wheel_url,
                    "packagetype": "bdist_wheel",
                    "digests": { "sha256": hex },
                }] },
            })
            .to_string();
            for path in [
                format!("/pypi/{}/json", dist.replace('_', "-")),
                format!("/pypi/{}/{}/json", dist.replace('_', "-"), version),
                // Underscored twin: normalization must never 404.
                // (Đường gạch dưới dự phòng — chuẩn hóa không được 404.)
                format!("/pypi/{dist}/json"),
                format!("/pypi/{dist}/{version}/json"),
            ] {
                mocks.push(
                    server
                        .mock("GET", path.as_str())
                        .with_status(200)
                        .with_header("content-type", "application/json")
                        .with_body(doc.clone())
                        .create(),
                );
            }
            mocks.push(
                server
                    .mock("GET", format!("/files/{filename}").as_str())
                    .with_status(200)
                    .with_header("content-type", "application/octet-stream")
                    .with_body(wheel)
                    .create(),
            );
        }
        Self {
            _server: server,
            _mocks: mocks,
            url,
        }
    }
}

#[test]
fn matrix_remove_rolls_back_hermetic_registry() {
    // Fully hermetic rollback (NO live PyPI): warm-install two fake
    // distributions from the fixture registry, break the artifact fetch,
    // remove with a cold store — the op must fail AND restore manifest
    // (both deps) plus byte-identical lock, with zero toolchain spawn.
    // (Rollback hermetic hoàn toàn — package giả, registry giả.)
    let project = TempDir::new().unwrap();
    lib_project(
        project.path(),
        "python",
        "pyproject.toml",
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = [\"mgc-race-pypkg-a==9.9.9\", \"mgc-race-pypkg-b==9.9.9\"]\n",
    );
    let pypi = HermeticPypi::new(&[("mgc_race_pypkg_a", "9.9.9"), ("mgc_race_pypkg_b", "9.9.9")]);
    let env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(&["install-lib"], project.path(), &env);
    assert_eq!(code, Some(0), "hermetic warm install must succeed:\n{out}");
    // Break the artifact fetch with a dead host; re-snapshot the
    // (tampered) lock — rollback must restore exactly these bytes.
    // (Phá fetch bằng host chết — rollback phải trả đúng từng byte.)
    let lock_path = project.path().join("mgc.lock");
    let lock_body = std::fs::read_to_string(&lock_path).unwrap();
    assert!(
        lock_body.contains(&pypi.url),
        "lock must reference the fixture registry (hermetic proof):\n{lock_body}"
    );
    let broken = lock_body.replacen(&pypi.url, "http://127.0.0.1:1", 1);
    assert_ne!(lock_body, broken, "fixture must actually break");
    std::fs::write(&lock_path, broken).unwrap();
    let lock_bytes_before = std::fs::read(&lock_path).unwrap();

    let cold = MatrixSandbox::new();
    let (code, out) = cold.run_with_env(&["remove-lib", "mgc-race-pypkg-b"], project.path(), &env);
    assert_ne!(
        code,
        Some(0),
        "remove whose install tail hits a dead registry must fail:\n{out}"
    );
    let manifest_after = std::fs::read_to_string(project.path().join("pyproject.toml")).unwrap();
    for dep in ["mgc-race-pypkg-a", "mgc-race-pypkg-b"] {
        assert!(
            manifest_after.contains(dep),
            "rolled-back manifest must still carry {dep}:\n{manifest_after}"
        );
    }
    assert_eq!(
        std::fs::read(&lock_path).unwrap(),
        lock_bytes_before,
        "rolled-back lock must be byte-identical"
    );
    cold.assert_no_spawn("hermetic remove-lib");
}
