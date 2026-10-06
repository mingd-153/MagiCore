//! AI build lifecycle tests — kiểm thử vòng đời build AI.\n+
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn find_mgc_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent()?.parent().map(|parent| parent.join("mgc")))
        })
        .expect("mgc binary path is unavailable")
}

#[cfg(unix)]
fn write_fake_python(bin: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let tool = bin.join("python");
    std::fs::write(
        &tool,
        "#!/bin/sh\nset -eu\ntest \"${PYTHON_OPTIMIZER_MARKER:-}\" = AI_BUILD_OPTIMIZED\nmkdir -p dist\nprintf verified > dist/optimizer-marker.txt\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&tool).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(tool, permissions).unwrap();
}

#[cfg(windows)]
fn write_fake_python(bin: &Path) {
    std::fs::write(
        bin.join("python.bat"),
        "@echo off\r\nif not \"%PYTHON_OPTIMIZER_MARKER%\"==\"AI_BUILD_OPTIMIZED\" exit /b 1\r\nmkdir dist\r\necho verified>dist\\optimizer-marker.txt\r\n",
    )
    .unwrap();
}

fn run_mgc_build(project: &Path, fake_bin: &Path) -> Output {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(fake_bin.to_path_buf()).chain(std::env::split_paths(&inherited)),
    )
    .unwrap();

    Command::new(find_mgc_binary())
        .arg("build")
        .current_dir(project)
        .env("PATH", path)
        .output()
        .expect("mgc build did not start")
}

#[test]
fn ai_python_build_uses_optimizer_env_and_creates_artifact() {
    let temp = TempDir::new().unwrap();
    let project = temp.path();
    let fake_bin = project.join("fake-bin");
    std::fs::create_dir(&fake_bin).unwrap();
    write_fake_python(&fake_bin);

    std::fs::write(project.join(".mgc.core"), "ai\n").unwrap();
    std::fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"ai-build-e2e\"\nversion = \"0.1.0\"\n\n[tool.magicore]\nframework = \"python-agent\"\n",
    )
    .unwrap();
    let optimizer = project.join(".mgc-optimizer");
    std::fs::create_dir(&optimizer).unwrap();
    std::fs::write(
        optimizer.join("pytorch_runtime.env"),
        "PYTHON_OPTIMIZER_MARKER=AI_BUILD_OPTIMIZED\n",
    )
    .unwrap();
    std::fs::write(optimizer.join("pytorch_docker.env"), "# empty\n").unwrap();

    let output = run_mgc_build(project, &fake_bin);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(output.status.success(), "mgc build failed:\n{combined}");
    assert!(
        project.join("dist/optimizer-marker.txt").is_file(),
        "AI build child did not receive optimizer env or create an artifact:\n{combined}"
    );
}
