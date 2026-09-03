use anyhow::{bail, Context, Result};
use colored::Colorize;
use mgc_config::project::ProjectExecutionConfig;
use mgc_ui::info;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::bundler::{Bundler, BundlerConfig};
use crate::context::ProjectContext;

pub async fn run(core: Option<&str>, target: Option<String>) -> Result<()> {
    let root = find_root()?;

    if !mgc_ui::is_quiet() {
        mgc_ui::blank_line();
        println!("📦 {}", "MagiCore Build".bold().cyan());
    }
    info(&format!("Project root: {}", root.display()));

    if root.join("Cargo.toml").exists() {
        // Load optimizer env for standalone Rust projects
        // Tải env optimizer cho Rust project độc lập
        let runtime = crate::commands::optimizer::runtime_detect::DetectedRuntime::RustLib;
        let optimizer_envs =
            crate::commands::optimizer::env_loader::load_optimizer_env(&root, &runtime)
                .map_err(|e| {
                    mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                    e
                })
                .unwrap_or_default();
        let rustflags = optimizer_envs.get("RUSTFLAGS").cloned();
        return build_rust_with_env(&root, rustflags);
    }

    let ctx = ProjectContext::load_with_core(core)?;
    info(&format!("Execution profile: {}", ctx.execution_summary()));
    match ctx.adapter().name() {
        "web" => build_web(&root, ctx.execution(), target).await,
        #[cfg(feature = "app")]
        "app" => build_app(&root).await,
        #[cfg(not(feature = "app"))]
        "app" => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "clo")]
        "cloud" => build_cloud(&root).await,
        #[cfg(not(feature = "clo"))]
        "cloud" => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "game")]
        "game" => build_game(&root).await,
        #[cfg(not(feature = "game"))]
        "game" => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "iot")]
        "iot" => build_iot(&root).await,
        #[cfg(not(feature = "iot"))]
        "iot" => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "lib")]
        "lib" => build_lib(&root).await,
        #[cfg(not(feature = "lib"))]
        "lib" => Err(crate::error::core_not_in_build("lib")),
        #[cfg(feature = "hardware")]
        "hardware" => build_hardware(&root).await,
        #[cfg(not(feature = "hardware"))]
        "hardware" => Err(crate::error::core_not_in_build("hardware")),
        #[cfg(feature = "ai")]
        "ai" => Err(crate::error::build_not_supported(
            "ai",
            "AI projects run with `mgc run` or `mgc dev` (05 §7)",
        )),
        #[cfg(not(feature = "ai"))]
        "ai" => Err(crate::error::core_not_in_build("ai")),
        other => bail!("'mgc build' not implemented for '{}' core yet", other),
    }
}

/// Game build routes only implemented engines — chỉ chạy engine đã có build contract.
#[cfg(feature = "game")]
async fn build_game(root: &Path) -> Result<()> {
    let engine = mgc_game_adapter::detect_engine(root);
    match engine {
        Some(mgc_game_adapter::GameEngine::Bevy) => {
            // Load optimizer env for Bevy (Rust) builds
            let runtime = crate::commands::optimizer::runtime_detect::DetectedRuntime::RustLib;
            let optimizer_envs =
                crate::commands::optimizer::env_loader::load_optimizer_env(root, &runtime)
                    .map_err(|e| {
                        mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                        e
                    })
                    .unwrap_or_default();
            let rustflags = optimizer_envs.get("RUSTFLAGS").cloned();
            build_rust_with_env(root, rustflags)
        }
        Some(mgc_game_adapter::GameEngine::Godot) => Err(crate::error::build_not_supported(
            "game/godot",
            "configure an export preset and run Godot export (03 §4 P2)",
        )),
        Some(mgc_game_adapter::GameEngine::Unity) => Err(crate::error::build_not_supported(
            "game/unity",
            "Unity batchmode export is not implemented yet (03 §4)",
        )),
        Some(mgc_game_adapter::GameEngine::Unreal) => Err(crate::error::build_not_supported(
            "game/unreal",
            "Unreal build is not implemented yet (03 §4 P2)",
        )),
        None => Err(crate::error::no_framework_detected("game engine", root)),
    }
}

/// IoT build (04 §5): esp32-rust → cargo (build_rust); platformio → pio run;
/// zephyr → west build. Toolchain thiếu → cảnh báo + hướng cài (04: không tự tải P1).
#[cfg(feature = "iot")]
async fn build_iot(root: &Path) -> Result<()> {
    if root.join("platformio.ini").exists() {
        if tool_unavailable("pio") {
            return Err(crate::error::build_toolchain_missing("pio"));
        }
        return run_allowlisted_tool(root, "pio", &["run"]);
    }
    if root.join("west.yml").exists() {
        if tool_unavailable("west") {
            return Err(crate::error::build_toolchain_missing("west"));
        }
        return run_allowlisted_tool(root, "west", &["build", "-b", "native_sim"]);
    }
    if root.join("Cargo.toml").exists() {
        // esp32-rust: build_rust with optimizer env
        // esp32-rust: build với env optimizer
        if tool_unavailable("cargo") {
            return Err(crate::error::build_toolchain_missing("cargo"));
        }

        // Load optimizer env for IoT Rust builds
        let runtime = crate::commands::optimizer::runtime_detect::DetectedRuntime::RustLib;
        let optimizer_envs =
            crate::commands::optimizer::env_loader::load_optimizer_env(root, &runtime)
                .map_err(|e| {
                    mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                    e
                })
                .unwrap_or_default();
        let rustflags = optimizer_envs.get("RUSTFLAGS").cloned();
        return build_rust_with_env(root, rustflags);
    }
    Err(crate::error::no_framework_detected("iot", root))
}

/// Lib build (09 §5): rust → cargo; ts → tsc qua node_modules/.bin (npm-format,
/// full resolver — không wrapper PM); python → python -m build (fail-closed nếu thiếu module build).
#[cfg(feature = "lib")]
async fn build_lib(root: &Path) -> Result<()> {
    // Load optimizer env for lib runtime
    // Tải env optimizer cho runtime thư viện
    let runtime = detect_lib_runtime(root);
    let optimizer_envs = crate::commands::optimizer::env_loader::load_optimizer_env(root, &runtime)
        .map_err(|e| {
            mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
            e
        })
        .unwrap_or_default();
    // Apply RUSTFLAGS for Rust builds
    // Áp dụng RUSTFLAGS cho Rust build
    let rustflags = optimizer_envs.get("RUSTFLAGS").cloned();

    if root.join("Cargo.toml").exists() {
        return build_rust_with_env(root, rustflags);
    }
    if root.join("pyproject.toml").exists() {
        if tool_unavailable("python") {
            return Err(crate::error::build_toolchain_missing("python"));
        }
        info("Building python lib: python -m build");

        // Load optimizer env for Python
        let env: Vec<(String, String)> = optimizer_envs.clone().into_iter().collect();
        let env_opt = if env.is_empty() { None } else { Some(env) };

        return run_allowlisted_tool_with_env(root, "python", &["-m", "build"], env_opt)
            .map_err(|e| crate::error::python_build_failed(&e));
    }
    let tsc = root.join("node_modules").join(".bin").join("tsc");
    if tsc.exists() {
        let args = node_bin_args(root, "tsc", &["-p", "tsconfig.json"])?
            .into_iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let local_bin = root.join("node_modules").join(".bin");

        // Merge PATH with optimizer env
        let mut env = vec![(
            "PATH".to_string(),
            prepend_path(&local_bin)?.to_string_lossy().to_string(),
        )];
        env.extend(optimizer_envs);

        info(&format!("tsc: node {}", args.join(" ")));
        let opts = mgc_exec::prelude::ExecOptions {
            cwd: Some(root.to_path_buf()),
            env,
            clean_env: false, // Preserve env with optimizer config
            ..Default::default()
        };
        return mgc_exec::prelude::run_inherited("node", &args, &opts).map(|_| ());
    }
    Err(crate::error::web_missing_executable(
        "tsc",
        "mgc install",
        root,
    ))
}

/// Hardware build: platformio (pio run) — chung cơ chế với iot.
#[cfg(feature = "hardware")]
async fn build_hardware(root: &Path) -> Result<()> {
    if root.join("platformio.ini").exists() {
        if tool_unavailable("pio") {
            return Err(crate::error::build_toolchain_missing("pio"));
        }
        return run_allowlisted_tool(root, "pio", &["run"]);
    }
    Err(crate::error::no_framework_detected("hardware", root))
}

/// C9 — build app: single framework passthrough hoặc multi-platform (shared + platforms).
/// Fail-closed: platform thiếu toolchain → cảnh báo + skip, không fail cả project.
#[cfg(feature = "app")]
async fn build_app(root: &Path) -> Result<()> {
    let mgc_toml = std::fs::read_to_string(root.join("mgc.toml")).ok();
    let v: Option<toml::Value> = mgc_toml.as_deref().and_then(|cfg| toml::from_str(cfg).ok());
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

    // Load optimizer env for app runtime
    let runtime = detect_app_runtime(root);
    let optimizer_envs = crate::commands::optimizer::env_loader::load_optimizer_env(root, &runtime)
        .map_err(|e| {
            mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
            e
        })
        .unwrap_or_default();
    let env: Vec<(String, String)> = optimizer_envs.into_iter().collect();
    let env_opt = if env.is_empty() { None } else { Some(env) };

    let (tool, args): (&str, &[&str]) = match language {
        "kotlin" => ("gradle", &["build"]),
        "swift" => ("swift", &["build"]),
        _ => ("flutter", &["build"]),
    };
    if tool_unavailable(tool) {
        return Err(crate::error::build_toolchain_missing(tool));
    }
    run_allowlisted_tool_with_env(root, tool, args, env_opt)?;
    mgc_ui::success(&format!("App build completed ({language})"));
    Ok(())
}

#[cfg(feature = "app")]
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

#[cfg(feature = "app")]
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
            mgc_ui::warning(&format!(
                "Platform '{platform}' has no directory — skipping"
            ));
            continue;
        }
        match platform.as_str() {
            "android" => {
                if tool_unavailable("gradle") {
                    mgc_ui::warning("gradle not found — skipping android build");
                    continue;
                }
                run_allowlisted_tool(&dir, "gradle", &["build"])?;
                built += 1;
            }
            "ios" => {
                if tool_unavailable("swift") {
                    mgc_ui::warning("swift not found — skipping ios build");
                    continue;
                }
                run_allowlisted_tool(&dir, "swift", &["build"])?;
                built += 1;
            }
            "react-native" => {
                mgc_ui::warning(
                    "react-native build is blocked in beta until the MagiCore-native app runner is available",
                );
            }
            "flutter" => {
                if tool_unavailable("flutter") {
                    mgc_ui::warning("flutter not found — skipping flutter build");
                    continue;
                }
                run_allowlisted_tool(&dir, "flutter", &["build"])?;
                built += 1;
            }
            other => mgc_ui::warning(&format!("Unknown platform '{other}' — skipping")),
        }
    }
    if built == 0 {
        return Err(crate::error::build_no_artifact());
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
#[cfg(feature = "clo")]
async fn build_cloud(root: &Path) -> Result<()> {
    let kind = mgc_cloud_adapter::detect_type(root)
        .ok_or_else(|| crate::error::no_framework_detected("cloud", root))?;
    match kind {
        mgc_cloud_adapter::CloudType::Cdk => {
            let bin = root.join("node_modules").join(".bin").join("cdk");
            if !bin.exists() {
                return Err(crate::error::web_missing_executable(
                    "cdk",
                    "mgc install",
                    root,
                ));
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
            let opts = mgc_exec::prelude::ExecOptions {
                cwd: Some(root.to_path_buf()),
                env,
                clean_env: true,
                ..Default::default()
            };
            mgc_exec::prelude::run_inherited("node", &args, &opts)?;
        }
        mgc_cloud_adapter::CloudType::Pulumi => {
            if tool_unavailable("pulumi") {
                return Err(crate::error::build_toolchain_missing("pulumi"));
            }
            run_allowlisted_tool(root, "pulumi", &["preview"])?;
        }
        mgc_cloud_adapter::CloudType::Terraform => {
            if tool_unavailable("terraform") {
                return Err(crate::error::build_toolchain_missing("terraform"));
            }
            run_allowlisted_tool(root, "terraform", &["plan"])?;
        }
        mgc_cloud_adapter::CloudType::Cloudflare => {
            return Err(crate::error::cloudflare_build_in_cicd_core());
        }
    }
    Ok(())
}

fn find_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if let Some(root) = mgc_config::project::ProjectConfig::find_project_root(&cwd) {
        return Ok(root);
    }
    Err(crate::error::no_project_found_build())
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
            info("MagiCore will build web assets first, then compile the Rust-native engine executable.");
        }
    }

    if run_framework_build_if_supported(root)? {
        let elapsed = start_time.elapsed();
        mgc_ui::blank_line();
        mgc_ui::success(&format!("Framework build completed in {:?}", elapsed));

        if matches!(
            resolved_target,
            WebBuildTarget::NativeReady | WebBuildTarget::CompiledExecutable
        ) {
            if let Some(engine_crate) = find_native_engine_crate(root) {
                let binary = build_native_engine(
                    root, // project root for optimizer config
                    &engine_crate,
                    matches!(resolved_target, WebBuildTarget::CompiledExecutable),
                )?;
                mgc_ui::success(&format!("Native engine binary ready: {}", binary.display()));
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
    mgc_ui::blank_line();
    mgc_ui::success(&format!(
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
                root, // project root for optimizer config
                &engine_crate,
                matches!(resolved_target, WebBuildTarget::CompiledExecutable),
            )?;
            mgc_ui::success(&format!("Native engine binary ready: {}", binary.display()));
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
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        env,
        clean_env: true,
        ..Default::default()
    };
    mgc_exec::prelude::run_inherited(&program.to_string_lossy(), &args, &opts)
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
    if let Some(pm) = mgc_exec::allowlist::find_forbidden_tool_in_script(script) {
        bail!(
            "Unsupported script '{}' in '{}': it delegates to '{}'. Core-web must execute natively through MagiCore or framework-local binaries, not through another package manager.",
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
            "Missing local executable '{}'. Run 'mgc install-web' in '{}'.",
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
    std::env::join_paths(parts).map_err(|err| crate::error::join_paths(&err))
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

fn build_native_engine(project_root: &Path, crate_dir: &Path, release: bool) -> Result<PathBuf> {
    let start = Instant::now();
    info(&format!(
        "Building native engine crate at {}...",
        crate_dir.display()
    ));

    // Load optimizer env from project root (not crate subdirectory)
    // Optimizer config is generated at project root: .mgc-optimizer/rust_cargo_profile.toml
    let runtime = crate::commands::optimizer::runtime_detect::DetectedRuntime::RustLib;
    let optimizer_envs =
        crate::commands::optimizer::env_loader::load_optimizer_env(project_root, &runtime)
            .map_err(|e| {
                mgc_ui::warning(&format!("Failed to load optimizer config: {}", e));
                e
            })
            .unwrap_or_default();
    let rustflags = optimizer_envs.get("RUSTFLAGS").cloned();
    let env_opt = rustflags.map(|flags| vec![("RUSTFLAGS".to_string(), flags)]);

    let mut args = vec!["build"];
    if release {
        args.push("--release");
    }

    run_allowlisted_tool_with_env(crate_dir, "cargo", &args, env_opt)?;

    let binary_name = if cfg!(windows) {
        "mgc-web-engine.exe"
    } else {
        "mgc-web-engine"
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
    run_allowlisted_tool_with_env(root, program, args, None)
}

fn run_allowlisted_tool_with_env(
    root: &Path,
    program: &str,
    args: &[&str],
    env: Option<Vec<(String, String)>>,
) -> Result<()> {
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        env: env.unwrap_or_default(),
        clean_env: false, // Preserve env when custom env provided
        ..Default::default()
    };
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    mgc_exec::prelude::run(program, &args, &opts)?;
    Ok(())
}

/// Detect lib runtime for optimizer env loading
/// Phát hiện runtime thư viện để load env optimizer
fn detect_lib_runtime(root: &Path) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
    use crate::commands::optimizer::runtime_detect::DetectedRuntime;

    if root.join("Cargo.toml").exists() {
        DetectedRuntime::RustLib
    } else if root.join("pyproject.toml").exists() {
        DetectedRuntime::PythonLib
    } else if root.join("go.mod").exists() {
        DetectedRuntime::GoLib
    } else if root.join("package.json").exists() {
        DetectedRuntime::TypeScriptLib
    } else {
        DetectedRuntime::Unknown
    }
}

/// Detect app runtime for optimizer env loading
/// Phát hiện runtime ứng dụng để load env optimizer
fn detect_app_runtime(root: &Path) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
    use crate::commands::optimizer::runtime_detect::DetectedRuntime;
    crate::commands::optimizer::runtime_detect::detect_runtimes(root, "app")
        .first()
        .cloned()
        .unwrap_or(DetectedRuntime::Unknown)
}

/// Build Rust with optional RUSTFLAGS from optimizer
/// Build Rust với RUSTFLAGS tùy chọn từ optimizer
fn build_rust_with_env(root: &Path, rustflags: Option<String>) -> Result<()> {
    let start = Instant::now();
    info("Detected Rust project — running cargo build...");

    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        env: rustflags
            .map(|flags| {
                mgc_ui::info(&format!("Applying RUSTFLAGS: {}", flags));
                vec![("RUSTFLAGS".to_string(), flags)]
            })
            .unwrap_or_default(),
        clean_env: false, // Preserve existing env
        ..Default::default()
    };
    mgc_exec::prelude::run("cargo", &["build".to_string()], &opts)?;

    let elapsed = start.elapsed();
    mgc_ui::success(&format!("Rust build completed in {:?}", elapsed));
    Ok(())
}

#[cfg(test)]
#[path = "../test/build_test.rs"]
mod tests;
