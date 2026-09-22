//! Native `add-lib` E2E: python/go/rust add MUST run through MGC itself
//! (resolve native + edit manifest mgc-side) with ZERO toolchain spawn.
//! Canary fakes on PATH prove no pip/cargo/go process ever runs.
//! Live registries (PyPI/crates.io/Go proxy) — online by design, like web_fw.
//! (Add-lib native E2E: add chạy trong MGC, không spawn toolchain.)

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

struct NoSpawnSandbox {
    bin_dir: TempDir,
    home_dir: TempDir,
    canary_log: std::path::PathBuf,
}

impl NoSpawnSandbox {
    fn multi(tools: &[&str]) -> Self {
        let bin_dir = TempDir::new().unwrap();
        let home_dir = TempDir::new().unwrap();
        let log_dir = bin_dir.path().join(".canary");
        std::fs::create_dir_all(&log_dir).unwrap();
        let log = log_dir.join("spawned.log");
        for tool in tools {
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

    fn run_mgc(&self, args: &[&str], cwd: &std::path::Path) -> (Option<i32>, String, String) {
        let mut cmd = Command::new(mgc_binary());
        cmd.args(args)
            .current_dir(cwd)
            .env("PATH", self.path_env())
            .env("HOME", self.home_dir.path())
            .env_remove("MGC_COMPAT_RUNTIME");
        // Real toolchains must be unreachable: canary shims shadow them,
        // and anything else must fail loudly, not silently succeed.
        let out = cmd.output().expect("spawn mgc");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
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

fn python_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativelib\"\necosystem = \"lib\"\n[lib]\nlanguage = \"python\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"nativelib\"\n",
    )
    .unwrap();
}

fn rust_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativelib\"\necosystem = \"lib\"\n[lib]\nlanguage = \"rust\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"nativelib\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
}

fn go_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativelib\"\necosystem = \"lib\"\n[lib]\nlanguage = \"go\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("go.mod"), "module nativelib\n\ngo 1.21\n").unwrap();
}

#[test]
fn python_add_lib_runs_inside_mgc_without_spawning_pip() {
    let project = TempDir::new().unwrap();
    python_lib_project(project.path());
    let sandbox = NoSpawnSandbox::multi(&["pip", "pip3", "uv", "python", "python3"]);

    let (code, stdout, stderr) = sandbox.run_mgc(&["add-lib", "six"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native python `add-lib six` must succeed:\n{output}"
    );
    let pyproject = std::fs::read_to_string(project.path().join("pyproject.toml")).unwrap();
    assert!(
        pyproject.contains("six"),
        "pyproject.toml must pin six (mgc-side edit):\n{pyproject}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("six") && lock.contains("sha256-"),
        "mgc.lock must record six with registry integrity:\n{lock}"
    );
    // Full native lifecycle: install fetches/verifies/unpacks with zero
    // spawn too.
    let (code, stdout, stderr) = sandbox.run_mgc(&["install-lib"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native python `install-lib` must succeed:\n{output}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn during install, got:\n{}",
        sandbox.marker_text()
    );
}

#[test]
fn rust_add_lib_runs_inside_mgc_without_spawning_cargo() {
    let project = TempDir::new().unwrap();
    rust_lib_project(project.path());
    let sandbox = NoSpawnSandbox::multi(&["cargo", "rustc"]);

    let (code, stdout, stderr) = sandbox.run_mgc(&["add-lib", "serde_json"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native rust `add-lib serde_json` must succeed:\n{output}"
    );
    let cargo = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(
        cargo.contains("serde_json"),
        "Cargo.toml must pin serde_json (mgc-side edit):\n{cargo}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("serde_json") && lock.contains("sha256-"),
        "mgc.lock must record serde_json with registry integrity:\n{lock}"
    );
    let (code, stdout, stderr) = sandbox.run_mgc(&["install-lib"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native rust `install-lib` must succeed:\n{output}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn during install, got:\n{}",
        sandbox.marker_text()
    );
}

#[test]
fn go_add_lib_runs_inside_mgc_without_spawning_go() {
    let project = TempDir::new().unwrap();
    go_lib_project(project.path());
    let sandbox = NoSpawnSandbox::multi(&["go"]);

    let (code, stdout, stderr) =
        sandbox.run_mgc(&["add-lib", "github.com/google/uuid"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native go `add-lib uuid` must succeed:\n{output}"
    );
    let gomod = std::fs::read_to_string(project.path().join("go.mod")).unwrap();
    assert!(
        gomod.contains("github.com/google/uuid"),
        "go.mod must require uuid (mgc-side edit):\n{gomod}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("github.com/google/uuid"),
        "mgc.lock must record uuid:\n{lock}"
    );
    assert!(
        lock.contains("sha256-") || lock.contains("sumdb-ziphash:h1:"),
        "mgc.lock must carry integrity evidence (proxy ziphash or sumdb record):\n{lock}"
    );
    let (code, stdout, stderr) = sandbox.run_mgc(&["install-lib"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native go `install-lib` must succeed:\n{output}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn during install, got:\n{}",
        sandbox.marker_text()
    );
}

fn dotnet_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativelib\"\necosystem = \"lib\"\n[lib]\nlanguage = \"dotnet\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("nativelib.csproj"),
        "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n  </PropertyGroup>\n</Project>\n",
    )
    .unwrap();
}

#[test]
fn dotnet_add_lib_runs_inside_mgc_without_spawning_dotnet() {
    let project = TempDir::new().unwrap();
    dotnet_lib_project(project.path());
    // No dotnet SDK on this machine on purpose: any toolchain spawn
    // would fail loudly instead of passing silently.
    let sandbox = NoSpawnSandbox::multi(&["dotnet"]);

    let (code, stdout, stderr) = sandbox.run_mgc(&["add-lib", "Newtonsoft.Json"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native dotnet `add-lib Newtonsoft.Json` must succeed:\n{output}"
    );
    let csproj = std::fs::read_to_string(project.path().join("nativelib.csproj")).unwrap();
    assert!(
        csproj.contains("Newtonsoft.Json"),
        "csproj must pin Newtonsoft.Json (mgc-side edit):\n{csproj}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("Newtonsoft.Json"),
        "mgc.lock must record Newtonsoft.Json:\n{lock}"
    );
    let (code, stdout, stderr) = sandbox.run_mgc(&["install-lib"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native dotnet `install-lib` must succeed:\n{output}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn during install, got:\n{}",
        sandbox.marker_text()
    );
}

fn java_lib_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativelib\"\necosystem = \"lib\"\n[lib]\nlanguage = \"java\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pom.xml"),
        "<project>\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>com.example</groupId>\n  <artifactId>nativelib</artifactId>\n  <version>0.1.0</version>\n</project>\n",
    )
    .unwrap();
}

#[test]
fn java_add_lib_runs_inside_mgc_without_spawning_maven() {
    let project = TempDir::new().unwrap();
    java_lib_project(project.path());
    // mvn EXISTS on this machine: the canary shim shadows it, so any
    // spawn is recorded instead of running for real.
    let sandbox = NoSpawnSandbox::multi(&["mvn", "java", "gradle"]);

    let (code, stdout, stderr) = sandbox.run_mgc(
        &["add-lib", "org.apache.commons:commons-lang3"],
        project.path(),
    );
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native java `add-lib commons-lang3` must succeed:\n{output}"
    );
    let pom = std::fs::read_to_string(project.path().join("pom.xml")).unwrap();
    assert!(
        pom.contains("commons-lang3"),
        "pom.xml must pin commons-lang3 (mgc-side edit):\n{pom}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("commons-lang3"),
        "mgc.lock must record commons-lang3:\n{lock}"
    );
    let (code, stdout, stderr) = sandbox.run_mgc(&["install-lib"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native java `install-lib` must succeed:\n{output}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn during install, got:\n{}",
        sandbox.marker_text()
    );
}
