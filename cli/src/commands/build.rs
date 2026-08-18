use anyhow::{bail, Context, Result};
use colored::Colorize;
use mg_config::project::ProjectExecutionConfig;
use mg_ui::info;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
        "app" => build_app(&root).await,
        "cloud" => build_cloud(&root).await,
        "game" => build_game(&root).await,
        "iot" => build_iot(&root).await,
        "lib" => build_lib(&root).await,
        "hardware" => build_hardware(&root).await,
        "ai" => {
            mg_ui::warning(
                "ai core không có build artifact (05 §7 — agent chạy qua `mg run`/`mg dev`); bỏ qua",
            );
            Ok(())
        }
        other => bail!("'mg build' not implemented for '{}' core yet", other),
    }
}

/// Game build (03 §4): bevy → cargo build (build_rust); godot/unity → P2
/// (godot --export / Unity batchmode) — cảnh báo, không fail.
async fn build_game(root: &Path) -> Result<()> {
    let engine = mg_game_adapter::detect_engine(root);
    match engine {
        Some(mg_game_adapter::GameEngine::Bevy) => build_rust(root),
        Some(mg_game_adapter::GameEngine::Godot) => {
            mg_ui::warning("godot export build là P2 (03 §4) — dùng editor export; bỏ qua build");
            Ok(())
        }
        Some(mg_game_adapter::GameEngine::Unity) => {
            mg_ui::warning("unity batchmode build là P1 chưa mở — scaffold-only; bỏ qua build");
            Ok(())
        }
        Some(mg_game_adapter::GameEngine::Unreal) => {
            mg_ui::warning("unreal build là P2 (03 §4) — scaffold-only; bỏ qua build");
            Ok(())
        }
        None => {
            mg_ui::warning("không detect được game engine (project.godot/Cargo.toml/.uproject)");
            Ok(())
        }
    }
}

/// IoT build (04 §5): esp32-rust → cargo (build_rust); platformio → pio run;
/// zephyr → west build. Toolchain thiếu → cảnh báo + hướng cài (04: không tự tải P1).
async fn build_iot(root: &Path) -> Result<()> {
    if root.join("platformio.ini").exists() {
        if tool_unavailable("pio") {
            mg_ui::warning("pio (platformio) not found — run `pip install platformio` first");
            return Ok(());
        }
        return run_allowlisted_tool(root, "pio", &["run"]);
    }
    if root.join("west.yml").exists() {
        if tool_unavailable("west") {
            mg_ui::warning("west not found — install Zephyr SDK + `pip install west` first");
            return Ok(());
        }
        return run_allowlisted_tool(root, "west", &["build", "-b", "native_sim"]);
    }
    if root.join("Cargo.toml").exists() {
        // esp32-rust: build_rust giữ cargo build; --target esp theo [iot] board là P1.5
        // (04 §5: cần espup toolchain — detect + lỗi rõ)
        if tool_unavailable("cargo") {
            mg_ui::warning("cargo not found — install Rust toolchain first");
            return Ok(());
        }
        return build_rust(root);
    }
    mg_ui::warning("không detect được iot framework (platformio.ini/west.yml/Cargo.toml)");
    Ok(())
}

/// Lib build (09 §5): rust → cargo; ts → tsc qua node_modules/.bin (npm-format,
/// full resolver — không wrapper PM); python → python -m build (fail-closed nếu thiếu module build).
async fn build_lib(root: &Path) -> Result<()> {
    if root.join("Cargo.toml").exists() {
        return build_rust(root);
    }
    if root.join("pyproject.toml").exists() {
        if tool_unavailable("python") {
            mg_ui::warning("python not found — install Python toolchain first");
            return Ok(());
        }
        info("Building python lib: python -m build");
        return run_allowlisted_tool(root, "python", &["-m", "build"]).map_err(|e| {
            anyhow::anyhow!(
                "python -m build failed: {e} — cài `pip install build` trong project này trước"
            )
        });
    }
    let tsc = root.join("node_modules").join(".bin").join("tsc");
    if tsc.exists() {
        let args = node_bin_args(root, "tsc", &["-p", "tsconfig.json"])?
            .into_iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let local_bin = root.join("node_modules").join(".bin");
        let env = vec![(
            "PATH".to_string(),
            prepend_path(&local_bin)?.to_string_lossy().to_string(),
        )];
        info(&format!("tsc: node {}", args.join(" ")));
        let opts = mg_exec::prelude::ExecOptions {
            cwd: Some(root.to_path_buf()),
            env,
            clean_env: true,
            ..Default::default()
        };
        return mg_exec::prelude::run_inherited("node", &args, &opts).map(|_| ());
    }
    mg_ui::warning("ts lib cần `mg install` trước (node_modules/.bin/tsc missing)");
    Ok(())
}

/// Hardware build: platformio (pio run) — chung cơ chế với iot.
async fn build_hardware(root: &Path) -> Result<()> {
    if root.join("platformio.ini").exists() {
        if tool_unavailable("pio") {
            mg_ui::warning("pio (platformio) not found — run `pip install platformio` first");
            return Ok(());
        }
        return run_allowlisted_tool(root, "pio", &["run"]);
    }
    mg_ui::warning("không detect được hardware framework (platformio.ini missing)");
    Ok(())
}

/// C9 — build app: single framework passthrough hoặc multi-platform (shared + platforms).
/// Fail-closed: platform thiếu toolchain → cảnh báo + skip, không fail cả project.
async fn build_app(root: &Path) -> Result<()> {
    let mg_toml = std::fs::read_to_string(root.join("mg.toml")).ok();
    let v: Option<toml::Value> = mg_toml.as_deref().and_then(|cfg| toml::from_str(cfg).ok());
    let language = v
        .as_ref()
        .and_then(|v| v.get("app"))
        .and_then(|a| a.get("language"))
        .and_then(|l| l.as_str())
        .or_else(|| infer_app_language(root))
        .unwrap_or("flutter");

    if language == "multi" {
        if let Some(v) = v {
            return build_multi_app(root, &v);
        }
    }

    let (tool, args): (&str, &[&str]) = match language {
        "kotlin" => ("gradle", &["build"]),
        "swift" => ("swift", &["build"]),
        _ => ("flutter", &["build"]),
    };
    if tool_unavailable(tool) {
        anyhow::bail!(
            "'{tool}' not found in PATH — install it first (mg doctor check lists required toolchains)"
        );
    }
    run_allowlisted_tool(root, tool, args)?;
    mg_ui::success(&format!("App build completed ({language})"));
    Ok(())
}

fn infer_app_language(root: &Path) -> Option<&'static str> {
    if root.join("Package.swift").exists() {
        Some("swift")
    } else if root.join("settings.gradle.kts").exists() || root.join("build.gradle.kts").exists() {
        Some("kotlin")
    } else if root.join("pubspec.yaml").exists() {
        Some("flutter")
    } else {
        None
    }
}

fn build_multi_app(root: &Path, v: &toml::Value) -> Result<()> {
    let platforms: Vec<String> = v
        .get("app")
        .and_then(|a| a.get("platforms"))
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let platforms = if platforms.is_empty() {
        vec![
            "android".to_string(),
            "ios".to_string(),
            "react-native".to_string(),
            "flutter".to_string(),
        ]
    } else {
        platforms
    };

    let mut built = 0;
    for platform in &platforms {
        let dir = root.join(platform);
        if !dir.exists() {
            mg_ui::warning(&format!(
                "Platform '{platform}' has no directory — skipping"
            ));
            continue;
        }
        match platform.as_str() {
            "android" => {
                if tool_unavailable("gradle") {
                    mg_ui::warning("gradle not found — skipping android build");
                    continue;
                }
                run_allowlisted_tool(&dir, "gradle", &["build"])?;
                built += 1;
            }
            "ios" => {
                if tool_unavailable("swift") {
                    mg_ui::warning("swift not found — skipping ios build");
                    continue;
                }
                run_allowlisted_tool(&dir, "swift", &["build"])?;
                built += 1;
            }
            "react-native" => {
                // npm chỉ cho phép trong RN subdir (C9 scoped exception, §5.2 giữ nguyên nơi khác)
                if tool_unavailable("npm") {
                    mg_ui::warning("npm not found — skipping react-native build");
                    continue;
                }
                run_allowlisted_tool(&dir, "npm", &["install"])?;
                built += 1;
            }
            "flutter" => {
                if tool_unavailable("flutter") {
                    mg_ui::warning("flutter not found — skipping flutter build");
                    continue;
                }
                run_allowlisted_tool(&dir, "flutter", &["build"])?;
                built += 1;
            }
            other => mg_ui::warning(&format!("Unknown platform '{other}' — skipping")),
        }
    }
    if built == 0 {
        mg_ui::warning("No platform built (toolchains missing or skipped)");
    }
    Ok(())
}

fn tool_unavailable(tool: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|dir| Path::new(dir).join(tool))
        .find(|p| p.is_file())
        .is_none()
}

/// Cloud build (06 §4): cdk synth (qua node_modules/.bin — npm-format, như web pattern),
/// pulumi preview, terraform plan — đều là dry-run (không ghi cloud state).
/// Fail-closed: toolchain thiếu → cảnh báo, không fail project.
async fn build_cloud(root: &Path) -> Result<()> {
    let kind = mg_cloud_adapter::detect_type(root)
        .ok_or_else(|| anyhow::anyhow!("No cloud framework detected in {}", root.display()))?;
    match kind {
        mg_cloud_adapter::CloudType::Cdk => {
            let bin = root.join("node_modules").join(".bin").join("cdk");
            if !bin.exists() {
                mg_ui::warning(
                    "cdk not installed — run `mg install` first (node_modules/.bin/cdk missing)",
                );
                return Ok(());
            }
            let args = node_bin_args(root, "cdk", &["synth"])?
                .into_iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect::<Vec<_>>();
            let local_bin = root.join("node_modules").join(".bin");
            let env = vec![(
                "PATH".to_string(),
                prepend_path(&local_bin)?.to_string_lossy().to_string(),
            )];
            info(&format!("cdk synth: node {}", args.join(" ")));
            let opts = mg_exec::prelude::ExecOptions {
                cwd: Some(root.to_path_buf()),
                env,
                clean_env: true,
                ..Default::default()
            };
            mg_exec::prelude::run_inherited("node", &args, &opts)?;
        }
        mg_cloud_adapter::CloudType::Pulumi => {
            if tool_unavailable("pulumi") {
                mg_ui::warning("pulumi not found — skipping build (install pulumi CLI first)");
                return Ok(());
            }
            run_allowlisted_tool(root, "pulumi", &["preview"])?;
        }
        mg_cloud_adapter::CloudType::Terraform => {
            if tool_unavailable("terraform") {
                mg_ui::warning(
                    "terraform not found — skipping build (install terraform CLI first)",
                );
                return Ok(());
            }
            run_allowlisted_tool(root, "terraform", &["plan"])?;
        }
        mg_cloud_adapter::CloudType::Cloudflare => {
            mg_ui::warning(
                "cloudflare workers build/deploy thuộc cicd core (Q12) — cloud core không làm P1",
            );
        }
    }
    Ok(())
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

    run_allowlisted_tool(root, "cargo", &["build"])?;

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
            let binary = build_native_engine(
                &engine_crate,
                matches!(resolved_target, WebBuildTarget::CompiledExecutable),
            )?;
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
    let mut env = vec![(
        "PATH".to_string(),
        prepend_path(&local_bin)?.to_string_lossy().to_string(),
    )];
    for (key, value) in envs {
        env.push((
            key.to_string_lossy().to_string(),
            value.to_string_lossy().to_string(),
        ));
    }

    let args = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        env,
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run_inherited(&program.to_string_lossy(), &args, &opts)
        .with_context(|| format!("failed to start build '{}'", program.display()))?;

    Ok(true)
}

type BuildLaunch = (PathBuf, Vec<OsString>, Vec<(OsString, OsString)>);

fn map_framework_build_script(root: &Path, tokens: &[&str]) -> Result<Option<BuildLaunch>> {
    let launch = match tokens {
        ["vite", "build"] => (
            node_runner(),
            node_bin_args(root, "vite", &["build"])?,
            vec![],
        ),
        ["vite", "build", rest @ ..] => (
            node_runner(),
            node_bin_args(root, "vite", &["build"])?
                .into_iter()
                .chain(rest.iter().map(OsString::from))
                .collect(),
            vec![],
        ),
        ["next", "build"] => (
            node_runner(),
            node_bin_args(root, "next", &["build"])?,
            vec![],
        ),
        ["next", "build", rest @ ..] => (
            node_runner(),
            node_bin_args(root, "next", &["build"])?
                .into_iter()
                .chain(rest.iter().map(OsString::from))
                .collect(),
            vec![],
        ),
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
        ["astro", "build"] => (
            node_runner(),
            node_bin_args(root, "astro", &["build"])?,
            vec![],
        ),
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

fn reject_external_package_manager_script(script: &str, manifest_path: &Path) -> Result<()> {
    if let Some(pm) = mg_exec::allowlist::find_forbidden_tool_in_script(script) {
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
    let bin = project_root
        .join("node_modules")
        .join(".bin")
        .join(bin_name);
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
        root.join("apps")
            .join("frontend")
            .join("crates")
            .join("engine"),
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

    let mut args = vec!["build"];
    if release {
        args.push("--release");
    }

    run_allowlisted_tool(crate_dir, "cargo", &args)?;

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

fn run_allowlisted_tool(root: &Path, program: &str, args: &[&str]) -> Result<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    mg_exec::prelude::run(program, &args, &opts)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_cloud, build_game, build_iot, build_lib, build_multi_app, find_native_engine_crate,
        map_framework_build_script, reject_external_package_manager_script,
        resolve_web_build_target, tool_unavailable, WebBuildTarget,
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
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname=\"mg-web-engine\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();

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
        assert_eq!(vite.1, {
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
        });

        let next = map_framework_build_script(dir.path(), &["next", "build"])
            .unwrap()
            .unwrap();
        assert_eq!(next.0, Path::new("node"));
        assert_eq!(next.1, {
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
        });
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

    #[test]
    fn framework_build_script_rejects_external_pm_wrappers_after_separator() {
        let err = reject_external_package_manager_script(
            "vite build && yarn install",
            Path::new("/tmp/package.json"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("delegates to 'yarn'"));
    }

    #[test]
    fn tool_unavailable_false_for_known_tool_in_path() {
        assert_eq!(tool_unavailable("sh"), false);
    }

    #[test]
    fn tool_unavailable_true_for_nonsense_tool() {
        assert_eq!(tool_unavailable("definitely-not-a-real-tool-mg"), true);
    }

    #[test]
    fn build_multi_skips_missing_platform_dir() {
        let tmp = std::env::temp_dir().join(format!("mg-build-multi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("mg.toml"), "[app]\nlanguage=\"multi\"\n").unwrap();
        let v: toml::Value =
            toml::from_str(&std::fs::read_to_string(tmp.join("mg.toml")).unwrap()).unwrap();
        build_multi_app(&tmp, &v).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_cloud_warns_skip_when_toolchain_missing() {
        let tmp = std::env::temp_dir().join(format!("mg-build-cloud-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // terraform: không có CLI trên máy → cảnh báo, không fail
        std::fs::write(tmp.join("mg.toml"), "[cloud]\ntype = \"terraform\"\n").unwrap();
        std::fs::write(tmp.join("main.tf"), "provider \"aws\" {}\n").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(build_cloud(&tmp)).unwrap();
        // cdk: node_modules/.bin/cdk thiếu → cảnh báo, không fail
        std::fs::write(tmp.join("mg.toml"), "[cloud]\ntype = \"cdk\"\n").unwrap();
        std::fs::write(
            tmp.join("package.json"),
            "{\"name\":\"x\",\"dependencies\":{\"aws-cdk-lib\":\"^2.0.0\"}}",
        )
        .unwrap();
        rt.block_on(build_cloud(&tmp)).unwrap();
        // pulumi: CLI thiếu → cảnh báo, không fail
        std::fs::write(tmp.join("mg.toml"), "[cloud]\ntype = \"pulumi\"\n").unwrap();
        std::fs::write(tmp.join("Pulumi.yaml"), "name: x\nruntime: nodejs\n").unwrap();
        rt.block_on(build_cloud(&tmp)).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn game_build_warns_skip_when_engine_p2() {
        let tmp = std::env::temp_dir().join(format!("mg-build-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("mg.toml"), "ecosystem = \"game\"\n").unwrap();
        std::fs::write(tmp.join("project.godot"), "[application]\n").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(build_game(&tmp)).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn iot_build_warns_skip_when_toolchain_missing() {
        let tmp = std::env::temp_dir().join(format!("mg-build-iot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("mg.toml"), "ecosystem = \"iot\"\n").unwrap();
        std::fs::write(
            tmp.join("platformio.ini"),
            "[env:esp32dev]\nplatform = espressif32\n",
        )
        .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(build_iot(&tmp)).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn lib_ts_build_warns_when_tsc_missing() {
        let tmp = std::env::temp_dir().join(format!("mg-build-libts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("mg.toml"), "ecosystem = \"lib\"\n").unwrap();
        std::fs::write(
            tmp.join("package.json"),
            "{\"name\":\"x\",\"version\":\"0.1.0\"}",
        )
        .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(build_lib(&tmp)).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
