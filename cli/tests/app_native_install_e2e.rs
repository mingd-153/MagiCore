//! Native `install-app` E2E: flutter install MUST run through MGC itself
//! (pubspec parse → pub.dev resolve → verified fetch → mgc.lock + pub
//! cache). ZERO `flutter` spawn — canary fakes prove it. Live pub.dev,
//! online by design.
//! (Install-app native E2E: chạy trong MGC, không spawn flutter.)

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

fn flutter_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativeapp\"\necosystem = \"app\"\n[app]\nlanguage = \"flutter\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pubspec.yaml"),
        "name: nativeapp\nenvironment:\n  sdk: \">=3.0.0 <4.0.0\"\ndependencies:\n  meta: ^1.12.0\n",
    )
    .unwrap();
}

#[test]
fn flutter_install_app_runs_inside_mgc_without_spawning_flutter() {
    let project = TempDir::new().unwrap();
    flutter_project(project.path());
    let sandbox = NoSpawnSandbox::multi(&["flutter", "dart"]);

    let (code, stdout, stderr) = sandbox.run_mgc(&["install-app"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native flutter `install-app` must succeed:\n{output}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("meta") && lock.contains("sha256-"),
        "mgc.lock must record meta with registry integrity:\n{lock}"
    );
}
