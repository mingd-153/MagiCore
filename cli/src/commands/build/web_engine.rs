//! `build/web_engine.rs` — Web build engine (P1 split from build.rs,
//! Tech Lead 2026-09-11). One responsibility: turning the resolved web
//! build lane into a real build — framework script mapping through the
//! compat gate, node_modules/.bin shims, and the native-engine compiled
//! executable lane. Per-core dispatch stays in build.rs.
//! `build/web_engine.rs` — Engine build web (tách từ build.rs theo P1).
//! Một trách nhiệm: biến lane build web đã resolve thành build thật —
//! ánh xạ script framework qua cổng compat, shim node_modules/.bin, và
//! lane executable compile native-engine. Dispatch per-core giữ ở
//! build.rs.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::bundler::{Bundler, BundlerConfig};
use mgc_config::project::ProjectExecutionConfig;
use mgc_ui::info;

pub(super) async fn build_web(
    root: &Path,
    execution: &ProjectExecutionConfig,
    target: Option<String>,
    compat: &crate::commands::compat::CompatMode,
) -> Result<()> {
    let start_time = Instant::now();

    let resolved_target = resolve_web_build_target(execution, target.as_deref());
    info(&format!("Resolved build lane: {}", resolved_target.label()));

    match resolved_target {
        WebBuildTarget::CompatibilityShell => {
            info("Engine Web: Running compatibility-shell bundler...");
        }
        WebBuildTarget::NativeReady => {
            info(
                "Engine Web: Running compatibility-shell build with native-ready bridge metadata...",
            );
            info(
                "Native-ready lane keeps framework compatibility while preparing Rust/native execution surfaces.",
            );
        }
        WebBuildTarget::CompiledExecutable => {
            info("Engine Web: Compiled executable lane selected.");
            info(
                "MagiCore will build web assets first, then compile the Rust-native engine executable.",
            );
        }
    }

    if run_framework_build_if_supported(root, compat)? {
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
                info(
                    "No native engine crate detected for this project; compatibility artifact is still ready.",
                );
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
            info(
                "No native engine crate detected for this project; compatibility artifact is still ready.",
            );
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebBuildTarget {
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

pub(crate) fn resolve_web_build_target(
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

fn run_framework_build_if_supported(
    root: &Path,
    compat: &crate::commands::compat::CompatMode,
) -> Result<bool> {
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
    // F-B fix (2026-09-10 audit): script build trỏ runtime đối thủ
    // (bun/deno) phải qua cổng compat TƯỜNG MINH — native fail hướng
    // migration thay vì bỏ qua âm thầm rồi báo build thành công.
    if let Some(program) = tokens.first() {
        crate::commands::compat::gate_runtime_spawn(compat, program)?;
    }
    let Some((program, args, envs)) = map_framework_build_script(root, &tokens)? else {
        // Script không map được (kể cả "deno task build") không còn bị
        // bỏ qua im lặng: nếu program là runtime đối thủ đã bị gate ở
        // trên (native fail / compat pass); chỉ script framework lạ mới
        // rơi vào đây và nhường lane cho native bundler.
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
    // P0-1: compat lane truyền runtime đã chọn (gate ở trên đã kiểm).
    let compat_runtime = match compat {
        crate::commands::compat::CompatMode::Native => None,
        crate::commands::compat::CompatMode::Explicit(runtime) => Some(runtime.clone()),
    };
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        env,
        clean_env: true,
        compat_runtime,
        ..Default::default()
    };
    mgc_exec::prelude::run_inherited(&program.to_string_lossy(), &args, &opts)
        .with_context(|| format!("failed to start build '{}'", program.display()))?;

    Ok(true)
}

type BuildLaunch = (PathBuf, Vec<OsString>, Vec<(OsString, OsString)>);

pub(crate) fn map_framework_build_script(
    root: &Path,
    tokens: &[&str],
) -> Result<Option<BuildLaunch>> {
    // Compat lane (P0-1): bun/deno build scripts spawn the rival runtime
    // DIRECTLY — only reachable when the compat gate above already passed
    // (native mode fails earlier at gate_runtime_spawn). No bun.pm usage.
    // Lane compat: script build bun/deno spawn runtime đối thủ TRỰC TIẾP —
    // chỉ đến được đây khi cổng compat phía trên đã pass.
    if let ["bun", rest @ ..] = tokens {
        return Ok(Some((
            PathBuf::from("bun"),
            rest.iter().map(OsString::from).collect(),
            vec![],
        )));
    }
    if let ["deno", rest @ ..] = tokens {
        return Ok(Some((
            PathBuf::from("deno"),
            rest.iter().map(OsString::from).collect(),
            vec![],
        )));
    }
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

pub(crate) fn reject_external_package_manager_script(
    script: &str,
    manifest_path: &Path,
) -> Result<()> {
    if let Some(pm) = mgc_exec::allowlist::find_forbidden_tool_in_script(script) {
        // P0-1 (2026-09-10): "bun run x"/"deno run|task x" là runtime usage —
        // gate_runtime_spawn phía caller đã xử lý bun/deno (native fail,
        // compat mở đúng runtime). Chỉ từ chối khi token là PM THẬT
        // (bun install/npm/pnpm/...).
        let tokens: Vec<&str> = script.split_whitespace().collect();
        let is_runtime_usage = matches!(
            tokens.as_slice(),
            ["bun", "run", ..] | ["deno", "run", ..] | ["deno", "task", ..]
        );
        if !is_runtime_usage {
            bail!(
                "Unsupported script '{}' in '{}': it delegates to '{}'. Core-web must execute natively through MagiCore or framework-local binaries, not through another package manager.",
                script,
                manifest_path.display(),
                pm
            );
        }
    }
    Ok(())
}

fn node_runner() -> PathBuf {
    PathBuf::from("node")
}

pub(crate) fn node_bin_args(
    project_root: &Path,
    bin_name: &str,
    args: &[&str],
) -> Result<Vec<OsString>> {
    let bin_dir = project_root.join("node_modules").join(".bin");
    let bin = bin_dir.join(bin_name);
    // Windows: npm-style .bin uses .cmd shims (create_bin_link writes them) —
    // the extensionless entry may not exist. Accept the shim variants.
    // Windows: .bin dùng shim .cmd — file gốc có thể không tồn tại, chấp nhận biến thể.
    let entry_path = if bin.exists() {
        bin.clone()
    } else {
        let shim = bin_dir.join(format!("{bin_name}.cmd"));
        if shim.exists() {
            shim
        } else {
            bail!(
                "Missing local executable '{}'. Run 'mgc install-web' in '{}'.",
                bin_name,
                project_root.display()
            );
        }
    };

    // Resolve symlink target when the entry is a real symlink (unix). On
    // Windows the npm .cmd shim wraps the real JS entry — parse the quoted
    // script path from the shim body (npm shims end with: "<js path>" %*)
    // so node receives the JS file, not the shim itself.
    // Unix: đọc đích symlink. Windows: shim .cmd bọc JS thật — trích đường
    // dẫn JS trong nháy kép từ thân shim để node chạy JS, không chạy shim.
    let entry = {
        #[cfg(unix)]
        {
            std::fs::read_link(&entry_path)
                .map(|target| {
                    if target.is_absolute() {
                        target
                    } else {
                        entry_path.parent().unwrap_or(project_root).join(target)
                    }
                })
                .unwrap_or_else(|_| entry_path.clone())
        }
        #[cfg(not(unix))]
        {
            let raw = std::fs::read_to_string(&entry_path).unwrap_or_default();
            let mut resolved: Option<PathBuf> = None;
            for line in raw.lines().rev() {
                // mgc/npm shim target line:  "...path..." %* — the quoted path
                // may be extensionless (typescript's bin/tsc), so accept any
                // quoted existing file on the last command line.
                // Dòng đích shim: "...đường dẫn..." %* — đích có thể không
                // đuôi file (bin/tsc), nhận mọi đường dẫn tồn tại trong nháy.
                if let Some(q1) = line.find('"')
                    && let Some(q2) = line[q1 + 1..].find('"')
                {
                    let candidate = PathBuf::from(&line[q1 + 1..q1 + 1 + q2]);
                    if candidate.is_file() {
                        resolved = Some(candidate);
                        break;
                    }
                }
            }
            match resolved {
                Some(path) if path.is_absolute() => path,
                Some(path) => entry_path.parent().unwrap_or(project_root).join(path),
                None => entry_path.clone(),
            }
        }
    };

    let mut result = vec![
        OsString::from("--preserve-symlinks"),
        OsString::from("--preserve-symlinks-main"),
        entry.into_os_string(),
    ];
    result.extend(args.iter().map(OsString::from));
    Ok(result)
}

pub(crate) fn prepend_path(local_bin: &Path) -> Result<OsString> {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut parts = vec![local_bin.as_os_str().to_os_string()];
    parts.extend(std::env::split_paths(&current).map(|path| path.into_os_string()));
    std::env::join_paths(parts).map_err(|err| crate::error::join_paths(&err))
}

pub(crate) fn find_entry_point(root: &Path) -> Result<PathBuf> {
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

    bail!(
        "Could not find entry point. Checked: src/index.ts, src/index.tsx, src/main.ts, src/main.tsx, and package.json main/module fields"
    )
}

pub(crate) fn find_native_engine_crate(root: &Path) -> Option<PathBuf> {
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

pub(crate) fn run_allowlisted_tool(root: &Path, program: &str, args: &[&str]) -> Result<()> {
    run_allowlisted_tool_with_env(root, program, args, None)
}

pub(crate) fn run_allowlisted_tool_with_env(
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
pub(crate) fn detect_lib_runtime(
    root: &Path,
) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
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
pub(crate) fn detect_app_runtime(
    root: &Path,
) -> crate::commands::optimizer::runtime_detect::DetectedRuntime {
    use crate::commands::optimizer::runtime_detect::DetectedRuntime;
    crate::commands::optimizer::runtime_detect::detect_runtimes(root, "app")
        .first()
        .cloned()
        .unwrap_or(DetectedRuntime::Unknown)
}

/// Build Rust with the complete optimizer environment — build Rust với toàn bộ env optimizer.
pub(crate) fn build_rust_with_env(
    root: &Path,
    optimizer_envs: std::collections::HashMap<String, String>,
) -> Result<()> {
    let start = Instant::now();
    info("Detected Rust project — running cargo build...");

    if let Some(rustflags) = optimizer_envs.get("RUSTFLAGS") {
        mgc_ui::info(&format!("Applying RUSTFLAGS: {rustflags}"));
    }

    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        env: optimizer_envs.into_iter().collect(),
        clean_env: false, // Preserve existing env
        ..Default::default()
    };
    mgc_exec::prelude::run("cargo", &["build".to_string()], &opts)?;

    let elapsed = start.elapsed();
    mgc_ui::success(&format!("Rust build completed in {:?}", elapsed));
    Ok(())
}
