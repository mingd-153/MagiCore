//! V1.2 Capability Registry matrix: EVERY native-owned (core, language,
//! operation) cell MUST execute with ZERO toolchain spawn. Delegated /
//! unsupported cells MUST fail closed (refuse + zero spawn) without
//! --compat-runtime. One table, executable proof — a lane labeled Native
//! that spawns fails this file (false-native canary).
//! (Ma trận registry V1.2: cell Native zero-spawn, cell khác fail-closed.)

#![allow(clippy::unwrap_used)]

use std::process::Command;
use std::sync::OnceLock;
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
    // Known package managers, build tools, runtimes, and shell launchers.
    // Dynamic or unlisted executables still require the static spawn audit.
    // (Canary phủ bộ tool đã biết; executable động phải qua static audit.)
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
    "cdk",
    "pulumi",
    "wrangler",
    "pio",
    "platformio",
    "west",
    "cmd",
    "powershell",
    "pwsh",
    "where",
    "bash",
    "sh",
    "conda",
    "mamba",
    "micromamba",
    "poetry",
    "pdm",
    "pipx",
    "uvx",
    "cmake",
    "make",
    "ninja",
    "msbuild",
    "javac",
    "node-gyp",
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

struct CanaryExecutable {
    _directory: TempDir,
    path: std::path::PathBuf,
}

fn canary_executable() -> &'static std::path::Path {
    static CANARY: OnceLock<CanaryExecutable> = OnceLock::new();
    CANARY
        .get_or_init(|| {
            let directory = TempDir::new().expect("create native canary build directory");
            let source = directory.path().join("native_tool_canary.rs");
            let executable = directory.path().join(if cfg!(windows) {
                "native_tool_canary.exe"
            } else {
                "native_tool_canary"
            });
            std::fs::write(
                &source,
                r#"
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let tool = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "unknown-tool".to_string());
    let directory = std::env::var_os("MGC_CANARY_LOG")
        .expect("MGC_CANARY_LOG must point to the isolated canary log directory");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_nanos();
    let marker = std::path::PathBuf::from(directory)
        .join(format!("{tool}-{}-{nonce}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
        .expect("create unique spawn marker");
    writeln!(file, "{tool}").expect("write spawn marker");
}
"#,
            )
            .expect("write native canary source");
            let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let output = Command::new(compiler)
                .arg("--edition=2021")
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .output()
                .expect("compile native process-spawn canary");
            assert!(
                output.status.success(),
                "compile native process-spawn canary failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            CanaryExecutable {
                _directory: directory,
                path: executable,
            }
        })
        .path
        .as_path()
}

impl MatrixSandbox {
    fn new() -> Self {
        let bin_dir = TempDir::new().unwrap();
        let home_dir = TempDir::new().unwrap();
        let log = bin_dir.path().join(".canary");
        std::fs::create_dir_all(&log).unwrap();
        let canary = canary_executable();
        for tool in ALL_TOOLS {
            let name = if cfg!(windows) {
                format!("{tool}.exe")
            } else {
                (*tool).to_string()
            };
            let executable = bin_dir.path().join(name);
            std::fs::copy(canary, &executable).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&executable).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&executable, perms).unwrap();
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
            .env("MGC_CANARY_LOG", self.canary_log.as_os_str())
            .env_remove("MGC_COMPAT_RUNTIME");
        #[cfg(windows)]
        cmd.env("USERPROFILE", self.home_dir.path())
            .env("APPDATA", self.home_dir.path().join("AppData/Roaming"))
            .env("LOCALAPPDATA", self.home_dir.path().join("AppData/Local"))
            .env("PATHEXT", ".COM;.EXE;.BAT;.CMD");
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
        let markers = std::fs::read_dir(&self.canary_log)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let value = std::fs::read_to_string(entry.path()).unwrap_or_default();
                format!("{} ({})", entry.file_name().to_string_lossy(), value.trim())
            })
            .collect::<Vec<_>>();
        assert!(
            markers.is_empty(),
            "{what}: ZERO toolchain spawn allowed, got:\n{}",
            markers.join("\n")
        );
    }
}

#[test]
fn native_canary_resolves_launchers_without_recording_arguments() {
    let sandbox = MatrixSandbox::new();
    let tools = ["cargo", "cmd", "powershell", "bash"];
    for tool in tools {
        let mut command = Command::new(tool);
        command
            .arg("sensitive-argument-must-not-be-recorded")
            .env("PATH", sandbox.path_env())
            .env("MGC_CANARY_LOG", &sandbox.canary_log);
        #[cfg(windows)]
        command.env("PATHEXT", ".COM;.EXE;.BAT;.CMD");
        let status = command
            .status()
            .unwrap_or_else(|error| panic!("resolve canary for {tool}: {error}"));
        assert!(status.success(), "canary for {tool} must exit successfully");
    }
    let entries = std::fs::read_dir(&sandbox.canary_log)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut recorded = entries
        .iter()
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect::<Vec<_>>();
    recorded.sort();
    assert_eq!(recorded, vec!["bash\n", "cargo\n", "cmd\n", "powershell\n"]);
    assert!(
        recorded
            .iter()
            .all(|record| !record.contains("sensitive-argument"))
    );
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
fn native_add_cell_with_env(
    setup: fn(&std::path::Path),
    add_cmd: &[&str],
    manifest_file: &str,
    pin: &str,
    lock_pin: &str,
    extra_env: &[(&str, &str)],
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(add_cmd, project.path(), extra_env);
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
fn native_install_cell_with_env(
    setup: fn(&std::path::Path),
    add_cmd: &[&str],
    install_cmd: &[&str],
    extra_env: &[(&str, &str)],
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(add_cmd, project.path(), extra_env);
    assert_eq!(
        code,
        Some(0),
        "setup {} must succeed:\n{out}",
        add_cmd.join(" ")
    );
    let (code, out) = sandbox.run_with_env(install_cmd, project.path(), extra_env);
    assert_eq!(
        code,
        Some(0),
        "{} must succeed:\n{out}",
        install_cmd.join(" ")
    );
    sandbox.assert_no_spawn(&install_cmd.join(" "));
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
    let pypi = HermeticPypi::new(&[("six", "1.17.0")]);
    let env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    native_add_cell_with_env(
        setup,
        &["add-lib", "six"],
        "pyproject.toml",
        "six",
        "six",
        &env,
    );
    native_install_cell_with_env(setup, &["add-lib", "six"], &["install-lib"], &env);
    // List is a spawn-free manifest read.
    {
        let project = TempDir::new().unwrap();
        setup(project.path());
        let sandbox = MatrixSandbox::new();
        let (code, out) = sandbox.run_with_env(&["add-lib", "six"], project.path(), &env);
        assert_eq!(code, Some(0), "setup add must succeed:\n{out}");
        let (code, out) = sandbox.run_with_env(&["list-lib"], project.path(), &env);
        assert_eq!(code, Some(0), "list-lib must succeed:\n{out}");
        assert!(out.contains("six"), "list must show six:\n{out}");
        sandbox.assert_no_spawn("list-lib");
    }
    // Frozen replays the lock with zero spawn.
    {
        let project = TempDir::new().unwrap();
        setup(project.path());
        let sandbox = MatrixSandbox::new();
        let (code, out) = sandbox.run_with_env(&["add-lib", "six"], project.path(), &env);
        assert_eq!(code, Some(0), "setup add must succeed:\n{out}");
        let (code, out) = sandbox.run_with_env(&["install", "--frozen"], project.path(), &env);
        assert_eq!(code, Some(0), "frozen install must succeed:\n{out}");
        sandbox.assert_no_spawn("install --frozen");
    }
    pypi.assert_required_routes_used();
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
    let registry = HermeticCrates::new();
    let env = [
        ("MGC_CRATES_INDEX_URL", registry.index_url.as_str()),
        ("MGC_CRATES_DOWNLOAD_URL", registry.download_url.as_str()),
    ];
    native_add_cell_with_env(
        setup,
        &["add-lib", "mgcfixture"],
        "Cargo.toml",
        "mgcfixture",
        "mgcfixture",
        &env,
    );
    native_install_cell_with_env(setup, &["add-lib", "mgcfixture"], &["install-lib"], &env);
}

#[test]
fn matrix_lib_go_add_install() {
    let setup = |d: &std::path::Path| {
        let (f, b) = go_manifest();
        lib_project(d, "go", f, &b);
    };
    let proxy = HermeticGo::new();
    let env = [
        ("MGC_GO_PROXY_URL", proxy.url.as_str()),
        ("MGC_GO_SUM_URL", proxy.url.as_str()),
    ];
    native_add_cell_with_env(
        setup,
        &["add-lib", "github.com/google/uuid"],
        "go.mod",
        "github.com/google/uuid",
        "github.com/google/uuid",
        &env,
    );
    native_install_cell_with_env(
        setup,
        &["add-lib", "github.com/google/uuid"],
        &["install-lib"],
        &env,
    );
    proxy.assert_all_routes_used();
}

#[test]
fn matrix_lib_dotnet_add_install() {
    let setup = |d: &std::path::Path| {
        let (f, b) = dotnet_manifest();
        lib_project(d, "dotnet", f, &b);
    };
    let registry = HermeticNuget::new();
    let env = [("MGC_NUGET_INDEX_URL", registry.index_url.as_str())];
    native_add_cell_with_env(
        setup,
        &["add-lib", "Newtonsoft.Json"],
        "m.csproj",
        "Newtonsoft.Json",
        "Newtonsoft.Json",
        &env,
    );
    native_install_cell_with_env(
        setup,
        &["add-lib", "Newtonsoft.Json"],
        &["install-lib"],
        &env,
    );
    registry.assert_all_routes_used();
}

#[test]
fn matrix_lib_java_add_install() {
    let setup = |d: &std::path::Path| {
        let (f, b) = java_manifest();
        lib_project(d, "java", f, &b);
    };
    let registry = HermeticMaven::new();
    let env = [("MGC_MAVEN_REPO_URL", registry.url.as_str())];
    native_add_cell_with_env(
        setup,
        &["add-lib", "org.apache.commons:commons-lang3"],
        "pom.xml",
        "commons-lang3",
        "commons-lang3",
        &env,
    );
    native_install_cell_with_env(
        setup,
        &["add-lib", "org.apache.commons:commons-lang3"],
        &["install-lib"],
        &env,
    );
    registry.assert_all_routes_used();
}

#[test]
fn matrix_ai_python_add_install() {
    let pypi = HermeticPypi::new(&[("six", "1.17.0")]);
    let env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    native_add_cell_with_env(
        ai_project,
        &["add-ai", "six"],
        "pyproject.toml",
        "six",
        "six",
        &env,
    );
    native_install_cell_with_env(ai_project, &["add-ai", "six"], &["install-ai"], &env);
    pypi.assert_required_routes_used();
}

#[test]
fn matrix_app_flutter_install() {
    let project = TempDir::new().unwrap();
    flutter_project(project.path());
    let sandbox = MatrixSandbox::new();
    let pub_registry = HermeticPub::new("meta", "1.13.0");
    let env = [("MGC_PUB_INDEX_URL", pub_registry.url.as_str())];
    let (code, out) = sandbox.run_with_env(&["install-app"], project.path(), &env);
    assert_eq!(code, Some(0), "install-app must succeed:\n{out}");
    sandbox.assert_no_spawn("native Flutter install-app");
    pub_registry.assert_all_routes_used();
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
    native_remove_cell_with_env(setup, add_cmd, remove_cmd, manifest_file, pin, &[]);
}

fn native_remove_cell_with_env(
    setup: fn(&std::path::Path),
    add_cmd: &[&str],
    remove_cmd: &[&str],
    manifest_file: &str,
    pin: &str,
    extra_env: &[(&str, &str)],
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(add_cmd, project.path(), extra_env);
    assert_eq!(
        code,
        Some(0),
        "setup {} must succeed:\n{out}",
        add_cmd.join(" ")
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
    let pypi = HermeticPypi::new(&[("six", "1.17.0")]);
    let python_env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    native_remove_cell_with_env(
        py,
        &["add-lib", "six"],
        &["remove-lib", "six"],
        "pyproject.toml",
        "six",
        &python_env,
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
    let nuget = HermeticNuget::new();
    let nuget_env = [("MGC_NUGET_INDEX_URL", nuget.index_url.as_str())];
    native_remove_cell_with_env(
        net,
        &["add-lib", "Newtonsoft.Json"],
        &["remove-lib", "Newtonsoft.Json"],
        "m.csproj",
        "Newtonsoft.Json",
        &nuget_env,
    );
    nuget.assert_all_routes_used();
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
fn matrix_ai_foreign_python_manifest_update_refuses_without_mutation() {
    let project = TempDir::new().unwrap();
    write(
        project.path(),
        "mgc.toml",
        "name = \"m\"\necosystem = \"ai\"\n[ai]\nframework = \"python-agent\"\n",
    );
    write(project.path(), "requirements.txt", "six==1.15.0\n");
    let manifest = project.path().join("requirements.txt");
    let before = std::fs::read(&manifest).unwrap();

    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["update-ai", "six"], project.path());
    assert_ne!(
        code,
        Some(0),
        "foreign Python lane must refuse update:\n{out}"
    );
    assert!(
        out.contains("does not yet own the complete native dependency lifecycle"),
        "refusal must identify missing MGC ownership, not an unrelated package error:\n{out}"
    );
    assert_eq!(
        std::fs::read(&manifest).unwrap(),
        before,
        "refused foreign-lane update must not rewrite the requirements file"
    );
    sandbox.assert_no_spawn("AI update with requirements.txt");
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
fn cloud_dev_and_deploy_fail_closed_without_provider_cli_spawn() {
    for framework in ["terraform", "cdk", "pulumi", "cloudflare"] {
        let project = TempDir::new().unwrap();
        write(
            project.path(),
            "mgc.toml",
            &format!(
                "name = \"cloud-e2e\"\necosystem = \"cloud\"\n[cloud]\ntype = \"{framework}\"\n"
            ),
        );
        if framework == "terraform" {
            write(project.path(), "main.tf", "terraform {}\n");
        }
        let sandbox = MatrixSandbox::new();
        for args in [
            vec!["--core", "clo", "dev"],
            vec!["deploy"],
            vec!["deploy", "--run"],
        ] {
            let (code, out) = sandbox.run(&args, project.path());
            assert_ne!(
                code,
                Some(0),
                "cloud/{framework} {} must not imply a native dev/deploy succeeded:\n{out}",
                args.join(" ")
            );
            assert!(
                out.contains("MagiCore-native provider engine"),
                "refusal must identify missing native ownership:\n{out}"
            );
            sandbox.assert_no_spawn(&format!("cloud/{framework} {}", args.join(" ")));
        }
    }
}

#[test]
fn capabilities_accepts_explicit_json_format_flag() {
    let cwd = TempDir::new().unwrap();
    let sandbox = MatrixSandbox::new();
    let (code, output) = sandbox.run(&["capabilities", "--json"], cwd.path());

    assert_eq!(
        code,
        Some(0),
        "mgc capabilities --json must succeed:\n{output}"
    );
    sandbox.assert_no_spawn("mgc capabilities --json");
    let document: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|error| panic!("capabilities must emit valid JSON: {error}\n{output}"));
    assert!(document["cores"].is_array());
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
    let ai = cores.iter().find(|row| row["core"] == "ai").unwrap();
    let ai_ownership = &ai["dependency_ownership"];
    assert_eq!(
        ai_ownership["operations"]["install"]["owner"], "unsupported",
        "AI must not claim a language-wide native owner"
    );
    assert_eq!(
        ai_ownership["manifest_requirements"]["python"], "mgc-pyproject",
        "AI native ownership must publish its required manifest condition"
    );
    assert_eq!(
        ai_ownership["manifest_variants"]["python/mgc-pyproject"]["install"]["owner"],
        "mgc-native"
    );
    for operation in ["add", "remove", "update", "list"] {
        assert_eq!(
            ai_ownership["manifest_variants"]["python/mgc-pyproject"][operation]["owner"],
            "mgc-native",
            "MGC-owned AI Python manifest should expose native {operation}"
        );
        assert_eq!(
            ai_ownership["manifest_variants"]["python/foreign-lock"][operation]["owner"],
            "unsupported",
            "foreign-lock AI Python must not expose native {operation}"
        );
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
fn matrix_python_cached_reinstall_survives_registry_outage() {
    // Warm install only from a hermetic local registry, then remove that
    // registry and point the client at a closed loopback port. This proves
    // the test itself never depends on public PyPI while validating cache
    // reuse. `cargo test --offline` does not disable network inside tests.
    // (Chỉ warm từ registry giả local, sau đó tắt registry.)
    let project = TempDir::new().unwrap();
    lib_project(
        project.path(),
        "python",
        "pyproject.toml",
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = []\n",
    );
    let pypi = HermeticPypi::new(&[("mgc_offline_reinstall_pypkg", "9.9.9")]);
    let registry = pypi.url.clone();
    let sandbox = MatrixSandbox::new();
    let online = [("MGC_PYPI_INDEX_URL", registry.as_str())];
    let (code, out) = sandbox.run_with_env(
        &[
            "add-lib",
            "mgc-offline-reinstall-pypkg",
            "--version",
            "9.9.9",
        ],
        project.path(),
        &online,
    );
    assert_eq!(code, Some(0), "hermetic add must succeed:\n{out}");
    let (code, out) = sandbox.run_with_env(&["install-lib"], project.path(), &online);
    assert_eq!(code, Some(0), "hermetic warm install must succeed:\n{out}");
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap();
    assert!(
        lock.contains(&registry),
        "lock must prove it was populated from the local fixture registry:\n{lock}"
    );
    drop(pypi);
    let (code, out) = sandbox.run_with_env(
        &["install-lib"],
        project.path(),
        &[("MGC_PYPI_INDEX_URL", "http://127.0.0.1:1")],
    );
    assert_eq!(
        code,
        Some(0),
        "cached reinstall after registry outage must succeed from the verified wheel cache:\n{out}"
    );
    sandbox.assert_no_spawn("cached install-lib during registry outage");

    let (code, out) = sandbox.run_with_env(
        &["install", "--offline"],
        project.path(),
        &[("MGC_PYPI_INDEX_URL", "http://127.0.0.1:1")],
    );
    assert_eq!(
        code,
        Some(0),
        "offline install must replay the verified Python lock and cached wheel without network:\n{out}"
    );
    sandbox.assert_no_spawn("offline native Python install");
    let (code, out) = sandbox.run_with_env(
        &["install-lib", "--offline"],
        project.path(),
        &[("MGC_PYPI_INDEX_URL", "http://127.0.0.1:1")],
    );
    assert_eq!(
        code,
        Some(0),
        "install-lib must preserve the explicit offline flag through per-core dispatch:\n{out}"
    );
    sandbox.assert_no_spawn("offline install-lib native Python");

    let lock_path = project.path().join("mgc.lock");
    let lock_before_mismatch = std::fs::read(&lock_path).unwrap();
    let manifest_path = project.path().join("pyproject.toml");
    let manifest_before_mismatch = std::fs::read(&manifest_path).unwrap();
    let manifest_text = String::from_utf8(manifest_before_mismatch.clone()).unwrap();
    let stale_manifest = manifest_text.replacen("9.9.9", "9.8.9", 1);
    assert_ne!(
        stale_manifest, manifest_text,
        "fixture must change the manifest pin"
    );
    std::fs::write(&manifest_path, stale_manifest).unwrap();
    let (code, out) = sandbox.run_with_env(
        &["install", "--offline"],
        project.path(),
        &[("MGC_PYPI_INDEX_URL", "http://127.0.0.1:1")],
    );
    assert_ne!(
        code,
        Some(0),
        "offline install must reject a stale lock:\n{out}"
    );
    assert!(
        out.to_lowercase().contains("lock") && !out.to_lowercase().contains("network"),
        "stale offline lock must fail as a lock error, not fall back to network resolution:\n{out}"
    );
    assert_eq!(
        std::fs::read(&lock_path).unwrap(),
        lock_before_mismatch,
        "stale offline install must preserve lock bytes"
    );
    std::fs::write(&manifest_path, &manifest_before_mismatch).unwrap();

    let cold_sandbox = MatrixSandbox::new();
    let cold_project = TempDir::new().unwrap();
    // Copy only inputs, leaving the package cache behind — chỉ chép đầu vào, bỏ cache cũ.
    for file in ["mgc.toml", "pyproject.toml", "mgc.lock"] {
        std::fs::copy(project.path().join(file), cold_project.path().join(file)).unwrap();
    }
    assert!(
        !cold_project.path().join(".magicore").exists(),
        "cold project must not inherit the warmed project-local package cache"
    );
    let lock_before = std::fs::read(cold_project.path().join("mgc.lock")).unwrap();
    let manifest_before = std::fs::read(cold_project.path().join("pyproject.toml")).unwrap();
    let (code, out) = cold_sandbox.run_with_env(
        &["install", "--offline"],
        cold_project.path(),
        &[("MGC_PYPI_INDEX_URL", "http://127.0.0.1:1")],
    );
    assert_ne!(
        code,
        Some(0),
        "offline install with a cold cache must fail instead of fetching:\n{out}"
    );
    assert!(
        out.contains("offline") && out.contains("cache"),
        "cold offline failure must explain the missing cache entry:\n{out}"
    );
    assert_eq!(
        std::fs::read(cold_project.path().join("mgc.lock")).unwrap(),
        lock_before
    );
    assert_eq!(
        std::fs::read(cold_project.path().join("pyproject.toml")).unwrap(),
        manifest_before
    );
    assert!(
        !cold_sandbox
            .home_dir
            .path()
            .join(".magicore/store/pypi")
            .exists(),
        "cold offline preflight must not create a package store"
    );
    cold_sandbox.assert_no_spawn("cold offline native Python install");
}

#[test]
fn offline_flag_is_not_silently_dropped_for_other_cores() {
    let project = TempDir::new().unwrap();
    write(
        project.path(),
        "mgc.toml",
        "name = \"offline-app\"\necosystem = \"app\"\n[app]\nlanguage = \"flutter\"\n",
    );
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run(&["install", "--offline"], project.path());
    assert_ne!(
        code,
        Some(0),
        "unsupported offline mode must fail explicitly:\n{out}"
    );
    assert!(
        out.to_lowercase()
            .contains("offline install is not implemented"),
        "unsupported offline mode must not be silently ignored:\n{out}"
    );
    sandbox.assert_no_spawn("unsupported offline install core");
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
    let pypi = HermeticPypi::new(&[("six", "1.17.0")]);
    let env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(&["add-lib", "six"], project.path(), &env);
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
    let pypi = HermeticPypi::new(&[("six", "1.17.0")]);
    let env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(&["add-lib", "six"], project.path(), &env);
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
fn native_update_cell_with_env(
    setup: fn(&std::path::Path),
    update_cmd: &[&str],
    manifest_file: &str,
    old_pin: &str,
    extra_env: &[(&str, &str)],
) {
    let project = TempDir::new().unwrap();
    setup(project.path());
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(update_cmd, project.path(), extra_env);
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
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmgcfixture = \"0.9.0\"\n",
    );
}

fn outdated_go(dir: &std::path::Path) {
    lib_project(
        dir,
        "go",
        "go.mod",
        "module m\n\ngo 1.21\n\nrequire github.com/google/uuid v0.9.0\n",
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
    let pypi = HermeticPypi::new(&[("six", "1.17.0")]);
    let python_env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    native_update_cell_with_env(
        outdated_python,
        &["update-lib", "six"],
        "pyproject.toml",
        "1.15.0",
        &python_env,
    );
    let crates = HermeticCrates::new();
    let rust_env = [
        ("MGC_CRATES_INDEX_URL", crates.index_url.as_str()),
        ("MGC_CRATES_DOWNLOAD_URL", crates.download_url.as_str()),
    ];
    native_update_cell_with_env(
        outdated_rust,
        &["update-lib", "mgcfixture"],
        "Cargo.toml",
        "0.9.0",
        &rust_env,
    );
}

#[test]
fn matrix_native_update_go_dotnet_java() {
    let proxy = HermeticGo::new();
    let go_env = [
        ("MGC_GO_PROXY_URL", proxy.url.as_str()),
        ("MGC_GO_SUM_URL", proxy.url.as_str()),
    ];
    native_update_cell_with_env(
        outdated_go,
        &["update-lib", "github.com/google/uuid"],
        "go.mod",
        "v0.9.0",
        &go_env,
    );
    let nuget = HermeticNuget::new();
    let nuget_env = [("MGC_NUGET_INDEX_URL", nuget.index_url.as_str())];
    native_update_cell_with_env(
        outdated_dotnet,
        &["update-lib", "Newtonsoft.Json"],
        "m.csproj",
        "13.0.1",
        &nuget_env,
    );
    let maven = HermeticMaven::new();
    let maven_env = [("MGC_MAVEN_REPO_URL", maven.url.as_str())];
    native_update_cell_with_env(
        outdated_java,
        &["update-lib", "org.apache.commons:commons-lang3"],
        "pom.xml",
        "3.12.0",
        &maven_env,
    );
    proxy.assert_resolution_routes_used();
    nuget.assert_resolution_routes_used();
    maven.assert_resolution_routes_used();
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
    let pub_registry = HermeticPub::new("meta", "1.13.0");
    let pub_env = [("MGC_PUB_INDEX_URL", pub_registry.url.as_str())];
    native_update_cell_with_env(
        outdated_flutter,
        &["update-app", "meta"],
        "pubspec.yaml",
        "1.9.1",
        &pub_env,
    );
    pub_registry.assert_metadata_route_used();
    let pypi = HermeticPypi::new(&[("six", "1.17.0")]);
    let pypi_env = [("MGC_PYPI_INDEX_URL", pypi.url.as_str())];
    native_update_cell_with_env(
        outdated_ai,
        &["update-ai", "six"],
        "pyproject.toml",
        "1.15.0",
        &pypi_env,
    );
    pypi.assert_metadata_routes_used();
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

/// Hermetic crates sparse index + archive for native Bevy/ESP32 CLI E2E.
/// Index và archive crates cục bộ để E2E Bevy/ESP32 không cần Internet.
struct HermeticCrates {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    index_url: String,
    download_url: String,
}

/// Hermetic GOPROXY fixture for native Go CLI E2E.
/// Fixture GOPROXY cục bộ cho E2E CLI Go native.
struct HermeticGo {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    url: String,
}

/// Hermetic NuGet v3 service index, registration, catalog, and package fixture.
/// Fixture cục bộ NuGet v3 cho E2E .NET, không gọi nuget.org.
struct HermeticNuget {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    index_url: String,
}

/// Hermetic Maven repository metadata, POM, checksum, and artifact fixture.
/// Fixture cục bộ Maven cho E2E Java, không gọi Maven Central.
struct HermeticMaven {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    url: String,
}

/// Hermetic pub.dev package API and archive fixture for Flutter E2E.
/// Fixture API + archive pub.dev cục bộ cho E2E Flutter.
struct HermeticPub {
    _server: mockito::ServerGuard,
    _mocks: Vec<mockito::Mock>,
    url: String,
}

fn build_test_pub_archive(name: &str, version: &str) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for (path, contents) in [
        (
            "pubspec.yaml".to_string(),
            format!("name: {name}\nversion: {version}\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n"),
        ),
        (
            format!("lib/{name}.dart"),
            "const fixture = true;\n".to_string(),
        ),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, contents.as_bytes())
            .expect("append fixture pub package file");
    }
    archive
        .into_inner()
        .expect("finish fixture pub tar")
        .finish()
        .expect("finish fixture pub gzip")
}

fn build_test_nuget_package() -> Vec<u8> {
    use std::io::Write;

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer
        .start_file("Newtonsoft.Json.nuspec", options)
        .unwrap();
    writer
        .write_all(b"<package><metadata><id>Newtonsoft.Json</id><version>13.0.3</version></metadata></package>")
        .unwrap();
    writer
        .start_file("lib/netstandard2.0/Newtonsoft.Json.dll", options)
        .unwrap();
    writer.write_all(b"test fixture assembly").unwrap();
    writer.finish().unwrap().into_inner()
}

impl HermeticNuget {
    fn new() -> Self {
        use base64::Engine as _;
        use sha2::{Digest, Sha512};

        let mut server = mockito::Server::new();
        let base = server.url();
        let index_url = format!("{base}/v3/index.json");
        let package = build_test_nuget_package();
        let package_hash =
            base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&package));
        let resources = serde_json::json!({
            "resources": [
                {"@id": format!("{base}/registration/"), "@type": "RegistrationsBaseUrl/3.6.0"},
                {"@id": format!("{base}/flat/"), "@type": "PackageBaseAddress/3.0.0"}
            ]
        });
        let catalog_url = format!("{base}/catalog/newtonsoft.json.13.0.3.json");
        let registration = serde_json::json!({
            "items": [{
                "items": [{
                    "catalogEntry": {
                        "@id": catalog_url,
                        "id": "Newtonsoft.Json",
                        "version": "13.0.3",
                        "listed": true
                    }
                }]
            }]
        });
        let mocks = vec![
            server
                .mock("GET", "/v3/index.json")
                .expect_at_least(1)
                .with_status(200)
                .with_body(resources.to_string())
                .create(),
            server
                .mock("GET", "/flat/newtonsoft.json/index.json")
                .expect_at_least(1)
                .with_status(200)
                .with_body(r#"{"versions":["13.0.3"]}"#)
                .create(),
            server
                .mock("GET", "/registration/newtonsoft.json/index.json")
                .expect_at_least(1)
                .with_status(200)
                .with_body(registration.to_string())
                .create(),
            server
                .mock("GET", "/catalog/newtonsoft.json.13.0.3.json")
                .expect_at_least(1)
                .with_status(200)
                .with_body(
                    serde_json::json!({
                        "packageHash": package_hash,
                        "packageHashAlgorithm": "SHA512"
                    })
                    .to_string(),
                )
                .create(),
            server
                .mock("GET", "/flat/newtonsoft.json/13.0.3/newtonsoft.json.nuspec")
                .expect_at_least(1)
                .with_status(200)
                .with_body("<package><metadata><dependencies /></metadata></package>")
                .create(),
            server
                .mock(
                    "GET",
                    "/flat/newtonsoft.json/13.0.3/Newtonsoft.Json.13.0.3.nupkg",
                )
                .expect_at_least(1)
                .with_status(200)
                .with_body(package)
                .create(),
        ];
        Self {
            _server: server,
            _mocks: mocks,
            index_url,
        }
    }

    fn assert_all_routes_used(&self) {
        for mock in &self._mocks {
            mock.assert();
        }
    }

    fn assert_resolution_routes_used(&self) {
        for mock in self._mocks.iter().take(5) {
            mock.assert();
        }
    }
}

impl HermeticMaven {
    fn new() -> Self {
        use sha2::{Digest, Sha256};

        let mut server = mockito::Server::new();
        let url = server.url();
        let jar = b"maven test artifact";
        let checksum = format!("{:x}", Sha256::digest(jar));
        let base = "/org/apache/commons/commons-lang3";
        let mocks = vec![
            server.mock("GET", format!("{base}/maven-metadata.xml").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body("<metadata><versioning><versions><version>3.12.0</version><version>3.13.0</version></versions></versioning></metadata>")
                .create(),
            server.mock("GET", format!("{base}/3.13.0/commons-lang3-3.13.0.pom").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body("<project><modelVersion>4.0.0</modelVersion><groupId>org.apache.commons</groupId><artifactId>commons-lang3</artifactId><version>3.13.0</version></project>")
                .create(),
            server.mock("GET", format!("{base}/3.13.0/commons-lang3-3.13.0.jar.sha256").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(checksum)
                .create(),
            server.mock("GET", format!("{base}/3.13.0/commons-lang3-3.13.0.jar").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(jar.as_slice())
                .create(),
        ];
        Self {
            _server: server,
            _mocks: mocks,
            url,
        }
    }

    fn assert_all_routes_used(&self) {
        for mock in &self._mocks {
            mock.assert();
        }
    }

    fn assert_resolution_routes_used(&self) {
        for mock in self._mocks.iter().take(3) {
            mock.assert();
        }
    }
}

impl HermeticPub {
    fn new(name: &str, version: &str) -> Self {
        use sha2::{Digest, Sha256};

        let mut server = mockito::Server::new();
        let url = server.url();
        let archive = build_test_pub_archive(name, version);
        let archive_hash = format!("{:x}", Sha256::digest(&archive));
        let archive_path = format!("/archives/{name}-{version}.tar.gz");
        let package_doc = serde_json::json!({
            "versions": [{
                "version": version,
                "archive_url": format!("{url}{archive_path}"),
                "archive_sha256": archive_hash,
                "pubspec": {
                    "environment": {"sdk": ">=3.0.0 <4.0.0"},
                    "dependencies": {}
                }
            }]
        });
        let mocks = vec![
            server
                .mock("GET", format!("/api/packages/{name}").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(package_doc.to_string())
                .create(),
            server
                .mock("GET", archive_path.as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(archive)
                .create(),
        ];
        Self {
            _server: server,
            _mocks: mocks,
            url,
        }
    }

    fn assert_metadata_route_used(&self) {
        self._mocks[0].assert();
    }

    fn assert_all_routes_used(&self) {
        for mock in &self._mocks {
            mock.assert();
        }
    }
}

fn build_test_go_module_zip(module: &str, version: &str) -> Vec<u8> {
    use std::io::Write;

    let prefix = format!("{module}@v{version}/");
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, contents) in [
        (
            format!("{prefix}go.mod"),
            format!("module {module}\n\ngo 1.21\n"),
        ),
        (
            format!("{prefix}uuid.go"),
            "package uuid\n\nfunc New() string { return \"fixture\" }\n".to_string(),
        ),
    ] {
        writer.start_file(path, options).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

impl HermeticGo {
    fn new() -> Self {
        use sha2::{Digest, Sha256};

        let module = "github.com/google/uuid";
        let version = "v1.0.0";
        let mut server = mockito::Server::new();
        let url = server.url();
        let zip = build_test_go_module_zip(module, version.trim_start_matches('v'));
        let ziphash = format!("{:x}", Sha256::digest(&zip));
        let base = format!("/{module}/@v");
        let mocks = vec![
            server
                .mock("GET", format!("{base}/list").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(format!("{version}\n"))
                .create(),
            server
                .mock("GET", format!("{base}/{version}.info").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(format!(
                    "{{\"Version\":\"{version}\",\"Time\":\"2024-01-01T00:00:00Z\"}}"
                ))
                .create(),
            server
                .mock("GET", format!("{base}/{version}.mod").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(format!("module {module}\n\ngo 1.21\n"))
                .create(),
            server
                .mock("GET", format!("{base}/{version}.ziphash").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(ziphash)
                .create(),
            server
                .mock("GET", format!("{base}/{version}.zip").as_str())
                .expect_at_least(1)
                .with_status(200)
                .with_body(zip)
                .create(),
        ];
        Self {
            _server: server,
            _mocks: mocks,
            url,
        }
    }

    fn assert_all_routes_used(&self) {
        for mock in &self._mocks {
            mock.assert();
        }
    }

    fn assert_resolution_routes_used(&self) {
        for mock in self._mocks.iter().take(3) {
            mock.assert();
        }
    }
}

fn build_test_crate_archive() -> Vec<u8> {
    let name = "mgcfixture";
    let version = "1.0.0";
    let manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n");
    let source = "pub fn fixture() {}\n";
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for (path, contents) in [
        (format!("{name}-{version}/Cargo.toml"), manifest.as_bytes()),
        (format!("{name}-{version}/src/lib.rs"), source.as_bytes()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, contents)
            .expect("append fixture crate file");
    }
    archive
        .into_inner()
        .expect("finish fixture crate tar")
        .finish()
        .expect("finish fixture crate gzip")
}

impl HermeticCrates {
    fn new() -> Self {
        use sha2::{Digest, Sha256};

        let mut server = mockito::Server::new();
        let index_url = server.url();
        let download_url = format!("{index_url}/api/v1/crates");
        let archive = build_test_crate_archive();
        let checksum = format!("{:x}", Sha256::digest(&archive));
        let index_entry = serde_json::json!({
            "name": "mgcfixture",
            "vers": "1.0.0",
            "deps": [],
            "cksum": checksum,
            "features": {},
            "yanked": false,
            "links": null,
        })
        .to_string();
        let mocks = vec![
            server
                .mock("GET", "/mg/cf/mgcfixture")
                .with_status(200)
                .with_header("content-type", "application/octet-stream")
                .with_body(index_entry)
                .create(),
            server
                .mock("GET", "/api/v1/crates/mgcfixture/1.0.0/download")
                .with_status(200)
                .with_header("content-type", "application/octet-stream")
                .with_body(archive)
                .create(),
        ];
        Self {
            _server: server,
            _mocks: mocks,
            index_url,
            download_url,
        }
    }
}

#[test]
fn matrix_game_bevy_add_install_native() {
    let project = TempDir::new().unwrap();
    bevy_project(project.path());
    let registry = HermeticCrates::new();
    let env = [
        ("MGC_CRATES_INDEX_URL", registry.index_url.as_str()),
        ("MGC_CRATES_DOWNLOAD_URL", registry.download_url.as_str()),
    ];
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(&["add-game", "mgcfixture"], project.path(), &env);
    assert_eq!(code, Some(0), "native bevy `add-game` must succeed:\n{out}");
    let body = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(
        body.contains("mgcfixture"),
        "Cargo.toml must pin mgcfixture:\n{body}"
    );
    sandbox.assert_no_spawn("add-game");
    let (code, out) = sandbox.run_with_env(&["install-game"], project.path(), &env);
    assert_eq!(
        code,
        Some(0),
        "native bevy `install-game` must succeed:\n{out}"
    );
    sandbox.assert_no_spawn("install-game");
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("mgcfixture"),
        "mgc.lock must record mgcfixture:\n{lock}"
    );
}

#[test]
fn matrix_iot_esp32_add_install_native() {
    let project = TempDir::new().unwrap();
    esp32_project(project.path());
    let registry = HermeticCrates::new();
    let env = [
        ("MGC_CRATES_INDEX_URL", registry.index_url.as_str()),
        ("MGC_CRATES_DOWNLOAD_URL", registry.download_url.as_str()),
    ];
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(&["add-iot", "mgcfixture"], project.path(), &env);
    assert_eq!(code, Some(0), "native esp32 `add-iot` must succeed:\n{out}");
    let body = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(
        body.contains("mgcfixture"),
        "Cargo.toml must pin mgcfixture:\n{body}"
    );
    sandbox.assert_no_spawn("add-iot");
    let (code, out) = sandbox.run_with_env(&["install-iot"], project.path(), &env);
    assert_eq!(
        code,
        Some(0),
        "native esp32 `install-iot` must succeed:\n{out}"
    );
    sandbox.assert_no_spawn("install-iot");
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("mgcfixture"),
        "mgc.lock must record mgcfixture:\n{lock}"
    );
}

#[test]
fn matrix_iot_platformio_build_refuses_external_package_build_tool() {
    let project = TempDir::new().unwrap();
    write(
        project.path(),
        "mgc.toml",
        "name = \"m\"\necosystem = \"iot\"\n[iot]\nframework = \"platformio\"\n",
    );
    write(
        project.path(),
        "platformio.ini",
        "[env:esp32dev]\nplatform = espressif32\n",
    );
    let sandbox = MatrixSandbox::new();
    let (code, output) = sandbox.run(&["build"], project.path());
    assert_ne!(
        code,
        Some(0),
        "PlatformIO build must not be reported as a MagiCore-native build:\n{output}"
    );
    assert!(
        output.contains("native PlatformIO build backend"),
        "the refusal must identify the missing MagiCore builder:\n{output}"
    );
    sandbox.assert_no_spawn("build iot/platformio");
}

#[test]
fn matrix_hardware_platformio_build_refuses_external_package_build_tool() {
    let project = TempDir::new().unwrap();
    write(
        project.path(),
        "mgc.toml",
        "name = \"m\"\necosystem = \"hardware\"\n",
    );
    write(
        project.path(),
        "platformio.ini",
        "[env:board]\nplatform = native\n",
    );
    let sandbox = MatrixSandbox::new();
    let (code, output) = sandbox.run(&["build"], project.path());
    assert_ne!(
        code,
        Some(0),
        "Hardware PlatformIO build must not be reported as MagiCore-native:\n{output}"
    );
    assert!(
        output.contains("native PlatformIO build backend"),
        "the refusal must identify the missing MagiCore builder:\n{output}"
    );
    sandbox.assert_no_spawn("build hardware/platformio");
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
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmgcfixture = \"0.9.0\"\n",
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
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmgcfixture = \"0.9.0\"\n",
    );
}

#[test]
fn matrix_native_update_game_iot() {
    let registry = HermeticCrates::new();
    let env = [
        ("MGC_CRATES_INDEX_URL", registry.index_url.as_str()),
        ("MGC_CRATES_DOWNLOAD_URL", registry.download_url.as_str()),
    ];
    native_update_cell_with_env(
        outdated_bevy,
        &["update-game", "mgcfixture"],
        "Cargo.toml",
        "0.9.0",
        &env,
    );
    native_update_cell_with_env(
        outdated_esp32,
        &["update-iot", "mgcfixture"],
        "Cargo.toml",
        "0.9.0",
        &env,
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
fn matrix_remove_rolls_back_on_injected_post_write_failure() {
    // Deterministic transaction proof: fail immediately after the manifest
    // write and before any network/install work; rollback must restore the
    // original manifest and leave no mutation journal behind.
    let project = TempDir::new().unwrap();
    let (f, _) = python_manifest();
    lib_project(
        project.path(),
        "python",
        f,
        "[project]\nname = \"m\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = [\"six==1.17.0\", \"attrs==23.1.0\"]\n",
    );
    let before = std::fs::read_to_string(project.path().join("pyproject.toml")).unwrap();
    assert!(
        before.contains("attrs"),
        "fixture must carry attrs:\n{before}"
    );
    let sandbox = MatrixSandbox::new();
    let (code, out) = sandbox.run_with_env(
        &["remove-lib", "attrs"],
        project.path(),
        &[("MGC_MUTATION_FAILPOINT", "after-manifest-write")],
    );
    assert_ne!(
        code,
        Some(0),
        "injected post-write failure must fail the remove:\n{out}"
    );
    assert!(
        out.contains("MGC_MUTATION_FAILPOINT"),
        "fault must be observable:\n{out}"
    );
    let after = std::fs::read_to_string(project.path().join("pyproject.toml")).unwrap();
    assert!(
        after.contains("attrs"),
        "rolled-back manifest must still carry attrs:\n{after}"
    );
    assert!(
        !project.path().join("mgc.lock").exists(),
        "rollback must preserve the prior absence of mgc.lock"
    );
    sandbox.assert_no_spawn("atomic remove-lib");
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

const PYPI_MOCKS_PER_PACKAGE: usize = 5;

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
                        .expect_at_least(1)
                        .with_status(200)
                        .with_header("content-type", "application/json")
                        .with_body(doc.clone())
                        .create(),
                );
            }
            mocks.push(
                server
                    .mock("GET", format!("/files/{filename}").as_str())
                    .expect_at_least(1)
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

    fn assert_metadata_routes_used(&self) {
        for package_mocks in self._mocks.chunks(PYPI_MOCKS_PER_PACKAGE) {
            assert_eq!(package_mocks.len(), PYPI_MOCKS_PER_PACKAGE);
            package_mocks[0].assert();
        }
    }

    fn assert_required_routes_used(&self) {
        for package_mocks in self._mocks.chunks(PYPI_MOCKS_PER_PACKAGE) {
            assert_eq!(package_mocks.len(), PYPI_MOCKS_PER_PACKAGE);
            package_mocks[0].assert();
            package_mocks[4].assert();
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
    let lock_path = project.path().join("mgc.lock");
    let lock_body = std::fs::read_to_string(&lock_path).unwrap();
    assert!(
        lock_body.contains(&pypi.url),
        "lock must reference the fixture registry (hermetic proof):\n{lock_body}"
    );
    let lock_bytes_before = std::fs::read(&lock_path).unwrap();

    let cold = MatrixSandbox::new();
    let mut env = env.to_vec();
    env.push(("MGC_MUTATION_FAILPOINT", "before-complete"));
    let (code, out) = cold.run_with_env(&["remove-lib", "mgc-race-pypkg-b"], project.path(), &env);
    assert_ne!(
        code,
        Some(0),
        "injected failure after the hermetic install tail must fail:\n{out}"
    );
    assert!(
        out.contains("MGC_MUTATION_FAILPOINT"),
        "fault must be observable:\n{out}"
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
