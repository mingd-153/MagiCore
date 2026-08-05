use anyhow::{bail, Context, Result};
use colored::Colorize;
use mg_config::project::ProjectExecutionConfig;
use mg_ui::info;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::bundler::{Bundler, BundlerConfig};
use crate::context::ProjectContext;

pub async fn run(core: Option<&str>, target: Option<String>) -> Result<()> {
    let root = find_root()?;

    if !mg_ui::is_quiet() {
        mg_ui::blank_line();
        println!("📦 {}", "MegaGate Build".bold().cyan());
    }
    info(&format!("Project root: {}", root.display()));

    if root.join("Cargo.toml").exists() {
        return build_rust(&root);
    }

    let ctx = ProjectContext::load_with_core(core)?;
    info(&format!("Execution profile: {}", ctx.execution_summary()));
    match ctx.adapter().name() {
        "web" => build_web(&root, ctx.execution(), target).await,
        other => bail!("'mg build' not implemented for '{}' core yet", other),
    }
}

fn find_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if let Some(root) = mg_config::project::ProjectConfig::find_project_root(&cwd) {
        return Ok(root);
    }
    anyhow::bail!("No project found (no mg.toml, package.json, or Cargo.toml)")
}

fn build_rust(root: &Path) -> Result<()> {
    let start = Instant::now();
    info("Detected Rust project — running cargo build...");

    let status = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(root)
        .status()?;

    if !status.success() {
        bail!("cargo build failed");
    }

    let elapsed = start.elapsed();
    mg_ui::success(&format!("Rust build completed in {:?}", elapsed));
    Ok(())
}

async fn build_web(
    root: &Path,
    execution: &ProjectExecutionConfig,
    target: Option<String>,
) -> Result<()> {
    let start_time = Instant::now();

    let resolved_target = resolve_web_build_target(execution, target.as_deref());
    info(&format!("Resolved build lane: {}", resolved_target.label()));

    match resolved_target {
        WebBuildTarget::CompatibilityShell => {
            info("Engine Web: Running compatibility-shell bundler...");
        }
        WebBuildTarget::NativeReady => {
            info("Engine Web: Running compatibility-shell build with native-ready bridge metadata...");
            info("Native-ready lane keeps framework compatibility while preparing Rust/native execution surfaces.");
        }
        WebBuildTarget::CompiledExecutable => {
            info("Engine Web: Compiled executable lane selected.");
            info("MegaGate will build web assets first, then compile the Rust-native engine executable.");
        }
    }

    if run_framework_build_if_supported(root)? {
        let elapsed = start_time.elapsed();
        mg_ui::blank_line();
        mg_ui::success(&format!("Framework build completed in {:?}", elapsed));

        if matches!(
            resolved_target,
            WebBuildTarget::NativeReady | WebBuildTarget::CompiledExecutable
        ) {
            if let Some(engine_crate) = find_native_engine_crate(root) {
                let binary = build_native_engine(
                    &engine_crate,
                    matches!(resolved_target, WebBuildTarget::CompiledExecutable),
                )?;
                mg_ui::success(&format!("Native engine binary ready: {}", binary.display()));
            } else {
                info("No native engine crate detected for this project; compatibility artifact is still ready.");
            }
        }

        return Ok(());
    }

    let entry = find_entry_point(root)?;
    info(&format!("Entry point: {}", entry.display()));

    let config = BundlerConfig {
        entry: entry.clone(),
        output_dir: root.join("dist"),
        minify: true,
        sourcemap: true,
        target: "es2020".to_string(),
        public_path: "/".to_string(),
    };

    let bundler = Bundler::new(config.clone());
    let result = bundler.bundle().await?;

    let elapsed = start_time.elapsed();
    mg_ui::blank_line();
    mg_ui::success(&format!(
        "Bundle created: {:.2} KB in {:?}",
        result.size as f64 / 1024.0,
        elapsed
    ));

    info("Processing assets...");
    crate::bundler::process_assets(&config).await?;

    if matches!(
        resolved_target,
        WebBuildTarget::NativeReady | WebBuildTarget::CompiledExecutable
    ) {
        if let Some(engine_crate) = find_native_engine_crate(root) {
            let binary = build_native_engine(&engine_crate, matches!(resolved_target, WebBuildTarget::CompiledExecutable))?;
            mg_ui::success(&format!("Native engine binary ready: {}", binary.display()));
        } else {
            info("No native engine crate detected for this project; compatibility artifact is still ready.");
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebBuildTarget {
    CompatibilityShell,
    NativeReady,
    CompiledExecutable,
}

impl WebBuildTarget {
    fn label(self) -> &'static str {
        match self {
            Self::CompatibilityShell => "compatibility-shell",
            Self::NativeReady => "native-ready",
            Self::CompiledExecutable => "compiled-executable",
        }
    }
}

fn resolve_web_build_target(
    execution: &ProjectExecutionConfig,
    explicit_target: Option<&str>,
) -> WebBuildTarget {
    if let Some(target) = explicit_target.map(|value| value.trim().to_ascii_lowercase()) {
        return match target.as_str() {
            "native" | "compiled" | "compiled-executable" | "executable" => {
                WebBuildTarget::CompiledExecutable
            }
            "native-ready" => WebBuildTarget::NativeReady,
            _ => WebBuildTarget::CompatibilityShell,
        };
    }

    match execution.lane.trim().to_ascii_lowercase().as_str() {
        "compiled-executable" => WebBuildTarget::CompiledExecutable,
        "native-ready" => WebBuildTarget::NativeReady,
        _ => WebBuildTarget::CompatibilityShell,
    }
}

fn run_framework_build_if_supported(root: &Path) -> Result<bool> {
    let package_json = root.join("package.json");
    if !package_json.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(&package_json)?;
    let package: Value = serde_json::from_str(&content)?;
    let Some(script) = package
        .get("scripts")
        .and_then(|scripts| scripts.get("build"))
        .and_then(|value| value.as_str())
    else {
        return Ok(false);
    };

    reject_external_package_manager_script(script, &package_json)?;
    let tokens: Vec<&str> = script.split_whitespace().collect();
    let Some((program, args, envs)) = map_framework_build_script(root, &tokens)? else {
        return Ok(false);
    };

    info(&format!(
        "Framework-aware build: {} {}",
        program.display(),
        args.iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    ));

    let local_bin = root.join("node_modules").join(".bin");
    let mut command = Command::new(&program);
    command
        .args(&args)
        .current_dir(root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("PATH", prepend_path(&local_bin)?);
    for (key, value) in envs {
        command.env(key, value);
    }

    let status = command
        .status()
        .with_context(|| format!("failed to start build '{}'", program.display()))?;
    if !status.success() {
        bail!("framework build exited with status {}", status);
    }

    Ok(true)
}

type BuildLaunch = (PathBuf, Vec<OsString>, Vec<(OsString, OsString)>);

fn map_framework_build_script(root: &Path, tokens: &[&str]) -> Result<Option<BuildLaunch>> {
    let launch = match tokens {
        ["vite", "build"] => (node_runner(), node_bin_args(root, "vite", &["build"])?, vec![]),
        ["vite", "build", rest @ ..] => {
            (node_runner(), node_bin_args(root, "vite", &["build"])?
                .into_iter()
                .chain(rest.iter().map(OsString::from))
                .collect(), vec![])
        }
        ["next", "build"] => (node_runner(), node_bin_args(root, "next", &["build"])?, vec![]),
        ["next", "build", rest @ ..] => {
            (node_runner(), node_bin_args(root, "next", &["build"])?
                .into_iter()
                .chain(rest.iter().map(OsString::from))
                .collect(), vec![])
        }
        ["nuxt", "build"] => (
            node_runner(),
            node_bin_args(root, "nuxt", &["build"])?,
            vec![
                (
                    OsString::from("NUXT_TELEMETRY_DISABLED"),
                    OsString::from("1"),
                ),
                (
                    OsString::from("NUXT_TELEMETRY_CONSENT"),
                    OsString::from("0"),
                ),
            ],
        ),
        ["astro", "build"] => (node_runner(), node_bin_args(root, "astro", &["build"])?, vec![]),
        ["remix", "vite:build"] => (
            node_runner(),
            node_bin_args(root, "remix", &["vite:build"])?,
            vec![],
        ),
        ["ng", "build"] => (
            node_runner(),
            node_bin_args(root, "ng", &["build"])?,
            vec![
                (OsString::from("NG_CLI_ANALYTICS"), OsString::from("false")),
                (OsString::from("CI"), OsString::from("1")),
            ],
        ),
        _ => return Ok(None),
    };

    Ok(Some(launch))
}

fn script_uses_external_package_manager(script: &str) -> Option<&'static str> {
    let first = script.split_whitespace().next()?.trim();
    match first {
        "npm" => Some("npm"),
        "pnpm" => Some("pnpm"),
        "bun" => Some("bun"),
        "yarn" => Some("yarn"),
        "npx" => Some("npx"),
        "bunx" => Some("bunx"),
        _ => None,
    }
}

fn reject_external_package_manager_script(script: &str, manifest_path: &Path) -> Result<()> {
    if let Some(pm) = script_uses_external_package_manager(script) {
        bail!(
            "Unsupported script '{}' in '{}': it delegates to '{}'. Core-web must execute natively through MegaGate or framework-local binaries, not through another package manager.",
            script,
            manifest_path.display(),
            pm
        );
    }
    Ok(())
}

fn node_runner() -> PathBuf {
    PathBuf::from("node")
}

fn node_bin_args(project_root: &Path, bin_name: &str, args: &[&str]) -> Result<Vec<OsString>> {
    let bin = project_root.join("node_modules").join(".bin").join(bin_name);
    if !bin.exists() {
        bail!(
            "Missing local executable '{}'. Run 'mg install-web' in '{}'.",
            bin_name,
            project_root.display()
        );
    }

    let entry = std::fs::read_link(&bin)
        .map(|target| {
            if target.is_absolute() {
                target
            } else {
                bin.parent().unwrap_or(project_root).join(target)
            }
        })
        .unwrap_or_else(|_| bin.clone());

    let mut result = vec![
        OsString::from("--preserve-symlinks"),
        OsString::from("--preserve-symlinks-main"),
        entry.into_os_string(),
    ];
    result.extend(args.iter().map(OsString::from));
    Ok(result)
}

fn prepend_path(local_bin: &Path) -> Result<OsString> {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut parts = vec![local_bin.as_os_str().to_os_string()];
    parts.extend(std::env::split_paths(&current).map(|path| path.into_os_string()));
    std::env::join_paths(parts).map_err(|err| anyhow::anyhow!(err))
}

fn find_entry_point(root: &Path) -> Result<PathBuf> {
    let candidates = [
        "src/index.ts",
        "src/index.tsx",
        "src/main.ts",
        "src/main.tsx",
        "src/app.ts",
        "src/app.tsx",
        "index.ts",
        "index.tsx",
        "main.ts",
        "main.tsx",
        "src/index.js",
        "src/index.jsx",
        "src/main.js",
        "src/main.jsx",
        "src/app.js",
        "src/app.jsx",
        "index.js",
        "index.jsx",
        "main.js",
        "main.jsx",
    ];

    for candidate in candidates {
        let path = root.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    let pkg_path = root.join("package.json");
    if pkg_path.exists() {
        let content = std::fs::read_to_string(&pkg_path)?;
        let pkg: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
            let path = root.join(main);
            if path.exists() {
                return Ok(path);
            }
        }
        if let Some(module) = pkg.get("module").and_then(|v| v.as_str()) {
            let path = root.join(module);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    bail!("Could not find entry point. Checked: src/index.ts, src/index.tsx, src/main.ts, src/main.tsx, and package.json main/module fields")
}

fn find_native_engine_crate(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("crates").join("engine"),
        root.join("apps").join("frontend").join("crates").join("engine"),
    ];

    candidates
        .into_iter()
        .find(|path| path.join("Cargo.toml").exists())
}

fn build_native_engine(crate_dir: &Path, release: bool) -> Result<PathBuf> {
    let start = Instant::now();
    info(&format!(
        "Building native engine crate at {}...",
        crate_dir.display()
    ));

    let mut command = Command::new("cargo");
    command.arg("build");
    if release {
        command.arg("--release");
    }
    command.current_dir(crate_dir);

    let status = command.status()?;
    if !status.success() {
        bail!("native engine cargo build failed");
    }

    let binary_name = if cfg!(windows) {
        "mg-web-engine.exe"
    } else {
        "mg-web-engine"
    };
    let profile_dir = if release { "release" } else { "debug" };
    let binary = crate_dir.join("target").join(profile_dir).join(binary_name);
    if !binary.exists() {
        bail!(
            "native engine build completed but binary '{}' was not found",
            binary.display()
        );
    }

    info(&format!(
        "Native engine build completed in {:?}",
        start.elapsed()
    ));
    Ok(binary)
}

#[cfg(test)]
mod tests {
    use super::{
        find_native_engine_crate, map_framework_build_script,
        reject_external_package_manager_script, resolve_web_build_target, WebBuildTarget,
    };
    use mg_config::project::ProjectExecutionConfig;
    use std::{fs, path::Path};

    fn execution(lane: &str) -> ProjectExecutionConfig {
        ProjectExecutionConfig {
            architecture: "rust-first".to_string(),
            lane: lane.to_string(),
            compatibility_layer: "ts".to_string(),
            native_targets: vec!["frontend-executable".to_string()],
        }
    }

    #[test]
    fn explicit_build_target_overrides_execution_lane() {
        let execution = execution("compatibility-shell");
        assert_eq!(
            resolve_web_build_target(&execution, Some("compiled-executable")),
            WebBuildTarget::CompiledExecutable
        );
        assert_eq!(
            resolve_web_build_target(&execution, Some("native-ready")),
            WebBuildTarget::NativeReady
        );
    }

    #[test]
    fn execution_lane_drives_default_build_target() {
        assert_eq!(
            resolve_web_build_target(&execution("compatibility-shell"), None),
            WebBuildTarget::CompatibilityShell
        );
        assert_eq!(
            resolve_web_build_target(&execution("native-ready"), None),
            WebBuildTarget::NativeReady
        );
        assert_eq!(
            resolve_web_build_target(&execution("compiled-executable"), None),
            WebBuildTarget::CompiledExecutable
        );
    }

    #[test]
    fn detects_native_engine_crate_in_frontend_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let crate_dir = dir.path().join("crates").join("engine");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname=\"mg-web-engine\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();

        assert_eq!(find_native_engine_crate(dir.path()), Some(crate_dir));
    }

    #[test]
    fn framework_build_script_maps_vite_and_next() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("vite"), "").unwrap();
        fs::write(bin_dir.join("next"), "").unwrap();

        let vite = map_framework_build_script(dir.path(), &["vite", "build"])
            .unwrap()
            .unwrap();
        assert_eq!(vite.0, Path::new("node"));
        assert_eq!(
            vite.1,
            {
                let args = vite
                    .1
                    .iter()
                    .map(|value| value.to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                assert_eq!(args[0], "--preserve-symlinks");
                assert_eq!(args[1], "--preserve-symlinks-main");
                assert!(args[2].contains("node_modules"));
                assert_eq!(args[3], "build");
                vite.1.clone()
            }
        );

        let next = map_framework_build_script(dir.path(), &["next", "build"])
            .unwrap()
            .unwrap();
        assert_eq!(next.0, Path::new("node"));
        assert_eq!(
            next.1,
            {
                let args = next
                    .1
                    .iter()
                    .map(|value| value.to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                assert_eq!(args[0], "--preserve-symlinks");
                assert_eq!(args[1], "--preserve-symlinks-main");
                assert!(args[2].contains("node_modules"));
                assert_eq!(args[3], "build");
                next.1.clone()
            }
        );
    }

    #[test]
    fn framework_build_script_rejects_external_pm_wrappers() {
        let err = reject_external_package_manager_script(
            "npm run build:inner",
            Path::new("/tmp/package.json"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("delegates to 'npm'"));
    }
}
