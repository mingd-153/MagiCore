use anyhow::{Result, bail};
use colored::Colorize;
use mgc_ui::info;
use std::path::{Path, PathBuf};

/// Prevent Go's compiler driver from downloading/updating dependency metadata
/// during a MagiCore build. Dependencies must already be present and verified
/// by the selected install path.
fn go_offline_env() -> Vec<(String, String)> {
    vec![
        ("GOPROXY".to_string(), "off".to_string()),
        ("GOSUMDB".to_string(), "off".to_string()),
        ("GOTOOLCHAIN".to_string(), "local".to_string()),
    ]
}

pub async fn run(
    core: Option<&str>,
    target: Option<String>,
    compat_runtime: Option<&str>,
) -> Result<()> {
    // Reject legacy rival-runtime flags before project discovery for every core.
    // (Từ chối cờ runtime đối thủ trước khi dò project, áp dụng đồng nhất mọi core.)
    let compat = crate::commands::compat::CompatMode::from_flag(compat_runtime)?;
    ensure_native_build_mode(&compat)?;
    let root = find_root()?;

    if !mgc_ui::is_quiet() {
        mgc_ui::blank_line();
        println!("📦 {}", "MagiCore Build".bold().cyan());
    }
    info(&format!("Project root: {}", root.display()));

    if should_use_legacy_rust_build(&root)? {
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
        return build_rust_with_env(&root, optimizer_envs);
    }

    let ctx = ProjectContext::load_with_core(core)?;
    info(&format!("Execution profile: {}", ctx.execution_summary()));
    match ctx.adapter().name() {
        "web" => build_web(&root, ctx.execution(), target, &compat).await,
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
        "ai" => build_ai(&root).await,
        #[cfg(not(feature = "ai"))]
        "ai" => Err(crate::error::core_not_in_build("ai")),
        other => bail!("'mgc build' not implemented for '{}' core yet", other),
    }
}

/// Build is a MagiCore-owned route; historical rival-runtime flags are refused.
/// (Build thuộc luồng MagiCore; cờ runtime đối thủ cũ luôn bị từ chối.)
fn ensure_native_build_mode(mode: &crate::commands::compat::CompatMode) -> Result<()> {
    match mode {
        crate::commands::compat::CompatMode::Native => Ok(()),
        crate::commands::compat::CompatMode::Explicit(runtime) => {
            Err(crate::error::rival_runtime_not_native(runtime))
        }
    }
}

/// Keep the legacy Cargo build lane only for a plain, unmarked Rust project.
/// Chỉ dùng nhánh Cargo legacy cho Rust project thuần, chưa có identity MagiCore.
fn should_use_legacy_rust_build(root: &Path) -> Result<bool> {
    let detected = mgc_config::project::ProjectConfig::detect_core(root)?;
    if detected.as_deref() != Some("lib") {
        return Ok(false);
    }
    let has_config =
        mgc_config::project::read_regular_project_text(&root.join("mgc.toml"), "project config")?
            .is_some();
    let has_marker = mgc_config::project::ProjectConfig::read_core_marker(root)?.is_some();
    Ok(!has_config && !has_marker)
}

/// A build backend selector accepts only regular project manifests.
/// Chỉ cho bộ chọn backend nhận manifest là file thường, không theo symlink.
fn project_manifest_present(root: &Path, name: &str) -> Result<bool> {
    Ok(mgc_config::project::read_regular_project_text(&root.join(name), name)?.is_some())
}

/// Build an AI project with its native language toolchain — build project AI bằng toolchain gốc.
#[cfg(feature = "ai")]
async fn build_ai(root: &Path) -> Result<()> {
    use crate::commands::optimizer::runtime_detect::DetectedRuntime;

    let runtime = if project_manifest_present(root, "pyproject.toml")? {
        DetectedRuntime::PythonPyTorch
    } else if project_manifest_present(root, "Cargo.toml")? {
        DetectedRuntime::RustCandle
    } else if project_manifest_present(root, "go.mod")? {
        DetectedRuntime::GoTensorFlow
    } else {
        return Err(crate::error::no_framework_detected("ai build", root));
    };
    let optimizer_envs = crate::commands::optimizer::env_loader::load_optimizer_env(root, &runtime)
        .map_err(|error| {
            mgc_ui::warning(&format!("Failed to load optimizer config: {error}"));
            error
        })
        .unwrap_or_default();

    match runtime {
        DetectedRuntime::PythonPyTorch => {
            let python = python_cmd();
            if tool_unavailable(python) {
                return Err(crate::error::build_toolchain_missing(python));
            }
            info("Building Python AI package: python -m build");
            let mut env = optimizer_envs.into_iter().collect::<Vec<_>>();
            crate::commands::python_runtime::extend_native_python_env(&mut env, root, false)?;
            let env = (!env.is_empty()).then_some(env);
            run_allowlisted_tool_with_env(root, python, &["-m", "build", "--no-isolation"], env)
                .map_err(|error| crate::error::python_build_failed(&error))?;
        }
        DetectedRuntime::RustCandle => {
            if tool_unavailable("cargo") {
                return Err(crate::error::build_toolchain_missing("cargo"));
            }
            build_rust_with_env(root, optimizer_envs)?;
        }
        DetectedRuntime::GoTensorFlow => {
            if tool_unavailable("go") {
                return Err(crate::error::build_toolchain_missing("go"));
            }
            let mut env = optimizer_envs.into_iter().collect::<Vec<_>>();
            env.extend(go_offline_env());
            run_allowlisted_tool_with_env(
                root,
                "go",
                &["build", "-mod=readonly", "./..."],
                Some(env),
            )?;
        }
        _ => unreachable!("AI build selects only an AI runtime"),
    }

    mgc_ui::success("AI build completed");
    Ok(())
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
            build_rust_with_env(root, optimizer_envs)
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

/// IoT build supports the MagiCore Rust lane only; provider builders may
/// perform dependency resolution and are not native MagiCore build backends.
/// IoT chỉ hỗ trợ lane Rust của MagiCore; builder ngoài có thể tự resolve
/// dependency nên không được coi là backend build native.
#[cfg(feature = "iot")]
async fn build_iot(root: &Path) -> Result<()> {
    if project_manifest_present(root, "platformio.ini")? {
        return Err(crate::error::build_not_supported(
            "iot/platformio",
            "MagiCore has no native PlatformIO build backend; refusing to invoke `pio run`, which may resolve or install dependencies",
        ));
    }
    if project_manifest_present(root, "west.yml")? {
        return Err(crate::error::build_not_supported(
            "iot/zephyr",
            "MagiCore has no native Zephyr build backend; refusing to invoke `west build`, which may resolve or install dependencies",
        ));
    }
    if project_manifest_present(root, "Cargo.toml")? {
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
        return build_rust_with_env(root, optimizer_envs);
    }
    Err(crate::error::no_framework_detected("iot", root))
}

/// First `*.csproj` directly inside the project root (SDK-style layout).
/// (File `*.csproj` đầu tiên ngay trong root project.)
#[cfg(feature = "lib")]
fn find_local_csproj(root: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("csproj"))
            && mgc_config::project::read_regular_project_text(&path, "C# project manifest")?
                .is_some()
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
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
    if project_manifest_present(root, "Cargo.toml")? {
        return build_rust_with_env(root, optimizer_envs);
    }
    if project_manifest_present(root, "pyproject.toml")? {
        let python = python_cmd();
        if tool_unavailable(python) {
            return Err(crate::error::build_toolchain_missing(python));
        }
        info("Building python lib: python -m build");

        // Load optimizer env for Python
        let env: Vec<(String, String)> = optimizer_envs.clone().into_iter().collect();
        let env_opt = if env.is_empty() { None } else { Some(env) };

        return run_allowlisted_tool_with_env(
            root,
            python,
            &["-m", "build", "--no-isolation"],
            env_opt,
        )
        .map_err(|e| crate::error::python_build_failed(&e));
    }
    // Go modules build through the go toolchain (mgc owns
    // resolve/fetch/install; compilation stays toolchain territory,
    // same split as the ai GoTensorFlow lane).
    if project_manifest_present(root, "go.mod")? {
        if tool_unavailable("go") {
            return Err(crate::error::build_toolchain_missing("go"));
        }
        info("Building go lib: go build ./...");
        let mut env: Vec<(String, String)> = optimizer_envs.into_iter().collect();
        env.extend(go_offline_env());
        return run_allowlisted_tool_with_env(
            root,
            "go",
            &["build", "-mod=readonly", "./..."],
            Some(env),
        )
        .map_err(|e| crate::error::go_build_failed(&e));
    }
    // .NET: dotnet SDK build (toolchain-gated; absent SDK fails closed
    // with guidance instead of a false native claim).
    if find_local_csproj(root)?.is_some() {
        if tool_unavailable("dotnet") {
            return Err(crate::error::build_toolchain_missing("dotnet"));
        }
        info("Building dotnet lib: dotnet build");
        let env: Vec<(String, String)> = optimizer_envs.into_iter().collect();
        let env_opt = if env.is_empty() { None } else { Some(env) };
        return run_allowlisted_tool_with_env(root, "dotnet", &["build", "--no-restore"], env_opt)
            .map_err(|e| crate::error::dotnet_build_failed(&e));
    }
    if has_java_build_manifest(root)? {
        return Err(crate::error::java_build_backend_unavailable(root));
    }
    let tsc = root.join("node_modules").join(".bin").join("tsc");
    // Windows: tsc resolves via the tsc.cmd shim (npm-style .bin layout).
    // Windows: tsc chạy qua shim tsc.cmd (bố cục .bin kiểu npm).
    let tsc_available = tsc.exists()
        || root
            .join("node_modules")
            .join(".bin")
            .join("tsc.cmd")
            .exists();
    if tsc_available {
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

/// Detect Java build descriptors before the TypeScript fallback.
/// Nhận diện build descriptor Java trước nhánh fallback TypeScript.
#[cfg(feature = "lib")]
fn has_java_build_manifest(root: &Path) -> Result<bool> {
    [
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ]
    .iter()
    .try_fold(false, |found, name| {
        if found {
            Ok(true)
        } else {
            project_manifest_present(root, name)
        }
    })
}

/// Hardware PlatformIO projects require a MagiCore-owned build backend.
/// Project PlatformIO của Hardware cần backend build do MagiCore sở hữu.
#[cfg(feature = "hardware")]
async fn build_hardware(root: &Path) -> Result<()> {
    if project_manifest_present(root, "platformio.ini")? {
        return Err(crate::error::build_not_supported(
            "hardware/platformio",
            "MagiCore has no native PlatformIO build backend; refusing to invoke `pio run`, which may resolve or install dependencies",
        ));
    }
    Err(crate::error::no_framework_detected("hardware", root))
}

/// C9 — build app: single framework or all selected multi-platform targets.
/// Missing, unknown, or unsupported targets make the aggregate build fail.
#[cfg(feature = "app")]
async fn build_app(root: &Path) -> Result<()> {
    let config = read_app_build_config(root)?;
    let language = resolve_app_language_from_config(root, config.as_ref())?;

    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if language == "multi"
        && let Some(v) = config.as_ref()
    {
        return build_multi_app(root, v);
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
        "kotlin" => ("gradle", &["build", "--offline"]),
        "swift" => (
            "swift",
            &["build", "--skip-update", "--disable-automatic-resolution"],
        ),
        // `flutter build web` is the universal CI target — no Android
        // SDK, no Xcode, runs on all three runners. `bundle` (the old
        // default) only produces asset dirs for mobile toolchains.
        // `flutter build web` là target CI phổ quát — không cần Android
        // SDK, không cần Xcode, chạy được cả ba runner. `bundle` (mặc
        // định cũ) chỉ sinh asset dir cho toolchain mobile.
        _ => ("flutter", &["build", "web", "--no-pub"]),
    };
    if tool_unavailable(tool) {
        return Err(crate::error::build_toolchain_missing(tool));
    }
    run_allowlisted_tool_with_env(root, tool, args, env_opt)?;
    mgc_ui::success(&format!("App build completed ({language})"));
    Ok(())
}

/// Resolve an App build target without guessing a default runtime. An invalid
/// project config or an unknown explicit language must not silently route a
/// project into Flutter's external build toolchain.
/// Không đoán runtime mặc định; config hỏng/không biết ngôn ngữ không được
/// âm thầm chạy Flutter.
#[cfg(all(test, feature = "app"))]
fn resolve_app_language(root: &Path) -> Result<&'static str> {
    let config = read_app_build_config(root)?;
    resolve_app_language_from_config(root, config.as_ref())
}

#[cfg(feature = "app")]
fn read_app_build_config(root: &Path) -> Result<Option<toml::Value>> {
    let path = root.join("mgc.toml");
    match mgc_config::project::read_regular_project_text(&path, "project config") {
        Ok(Some(contents)) => Ok(Some(
            toml::from_str::<toml::Value>(&contents)
                .map_err(|error| crate::error::app_build_config_invalid(&path, &error))?,
        )),
        Ok(None) => Ok(None),
        Err(error) => Err(crate::error::app_build_config_read_failed(&path, &error)),
    }
}

#[cfg(feature = "app")]
fn resolve_app_language_from_config(
    root: &Path,
    config: Option<&toml::Value>,
) -> Result<&'static str> {
    if let Some(language) = config
        .and_then(|value| value.get("app"))
        .and_then(|app| app.get("language"))
    {
        let language = language
            .as_str()
            .ok_or_else(|| crate::error::app_build_language_invalid("<non-string>"))?;
        return match language {
            "flutter" => Ok("flutter"),
            "kotlin" => Ok("kotlin"),
            "swift" => Ok("swift"),
            "multi" => Ok("multi"),
            other => Err(crate::error::app_build_language_invalid(other)),
        };
    }
    infer_app_language(root)?
        .ok_or_else(|| crate::error::no_framework_detected("app language", root))
}

#[cfg(feature = "app")]
fn infer_app_language(root: &Path) -> Result<Option<&'static str>> {
    let candidates = [
        ("Package.swift", "swift"),
        ("settings.gradle", "kotlin"),
        ("settings.gradle.kts", "kotlin"),
        ("build.gradle", "kotlin"),
        ("build.gradle.kts", "kotlin"),
        ("pubspec.yaml", "flutter"),
    ];
    let mut found = Vec::new();
    for (marker, language) in candidates {
        if project_manifest_present(root, marker)? && !found.contains(&language) {
            found.push(language);
        }
    }
    match found.as_slice() {
        [] => Ok(None),
        [language] => Ok(Some(*language)),
        _ => Err(crate::error::app_build_language_ambiguous(&found)),
    }
}

#[cfg(feature = "app")]
fn build_multi_app(root: &Path, v: &toml::Value) -> Result<()> {
    // Validate every configured target before invoking any toolchain.
    // Kiểm tra toàn bộ target trước khi gọi bất kỳ toolchain nào.
    let platforms = resolve_multi_platforms(v)?;

    let mut built = 0;
    let mut skipped = Vec::new();
    for platform in &platforms {
        let dir = root.join(platform);
        if !dir.exists() {
            skipped.push(format!("{platform} (directory missing)"));
            continue;
        }
        match platform.as_str() {
            "android" => {
                if tool_unavailable("gradle") {
                    skipped.push("android (gradle not found)".to_string());
                    continue;
                }
                run_allowlisted_tool(&dir, "gradle", &["build", "--offline"])?;
                built += 1;
            }
            "ios" => {
                if tool_unavailable("swift") {
                    skipped.push("ios (swift not found)".to_string());
                    continue;
                }
                run_allowlisted_tool(
                    &dir,
                    "swift",
                    &["build", "--skip-update", "--disable-automatic-resolution"],
                )?;
                built += 1;
            }
            "react-native" => {
                skipped.push("react-native (native runner unavailable)".to_string());
            }
            "flutter" => {
                if tool_unavailable("flutter") {
                    skipped.push("flutter (flutter toolchain not found)".to_string());
                    continue;
                }
                // `flutter build` without a target exits 2 ("Missing
                // target"). `web` is the universal desktop-CI target —
                // no Android SDK, no Xcode, works on all three runners
                // (P0 finding, 2026-09-12: the honest lifecycle matrix
                // caught the old bare `flutter build` failing).
                // `flutter build` thiếu target thì exit 2 ("Missing
                // target"). `web` là target phổ quát cho CI desktop —
                // không cần Android SDK, không cần Xcode, chạy được cả
                // ba runner (P0 finding 2026-09-12: matrix lifecycle
                // trung thực bắt được `flutter build` trần fail).
                run_allowlisted_tool(&dir, "flutter", &["build", "web", "--no-pub"])?;
                built += 1;
            }
            other => skipped.push(format!("{other} (unknown platform)")),
        }
    }
    finish_multi_build(built, &skipped)
}

#[cfg(feature = "app")]
fn resolve_multi_platforms(config: &toml::Value) -> Result<Vec<String>> {
    let Some(value) = config.get("app").and_then(|app| app.get("platforms")) else {
        return Ok(["android", "ios", "react-native", "flutter"]
            .into_iter()
            .map(String::from)
            .collect());
    };

    let Some(platforms) = value.as_array() else {
        return Err(crate::error::app_build_platforms_invalid(
            "expected an array of platform names",
        ));
    };
    if platforms.is_empty() {
        return Err(crate::error::app_build_platforms_invalid(
            "the explicit platform list must not be empty",
        ));
    }

    let mut selected = Vec::with_capacity(platforms.len());
    for (index, platform) in platforms.iter().enumerate() {
        let Some(platform) = platform.as_str() else {
            return Err(crate::error::app_build_platforms_invalid(&format!(
                "entry {index} must be a string"
            )));
        };
        if !["android", "ios", "react-native", "flutter"].contains(&platform) {
            return Err(crate::error::app_build_platforms_invalid(&format!(
                "unknown platform '{platform}' at entry {index}"
            )));
        }
        if selected.iter().any(|existing| existing == platform) {
            return Err(crate::error::app_build_platforms_invalid(&format!(
                "duplicate platform '{platform}'"
            )));
        }
        selected.push(platform.to_string());
    }
    Ok(selected)
}

#[cfg(feature = "app")]
fn finish_multi_build(built: usize, skipped: &[String]) -> Result<()> {
    if !skipped.is_empty() {
        return Err(crate::error::build_multi_platforms_incomplete(skipped));
    }
    if built == 0 {
        return Err(crate::error::build_no_artifact());
    }
    Ok(())
}

/// PATH lookup with Windows PATHEXT awareness: on top of the exact
/// name, `<tool><ext>` is accepted for every extension in PATHEXT
/// (`.BAT`/`.CMD`/`.EXE` wrappers like `flutter.bat`). PATHEXT is read
/// whenever present (Windows always sets it) so the lookup is
/// unit-testable on every OS; without it, Windows falls back to the
/// classic default set and other platforms check the exact name only.
/// (Tìm PATH có nhận biết PATHEXT Windows.)
pub(crate) fn tool_unavailable(tool: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return true;
    };
    let extensions: Vec<String> = std::env::var_os("PATHEXT")
        .and_then(|value| value.into_string().ok())
        .map(|value| {
            value
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_else(|| {
            #[cfg(windows)]
            {
                vec![
                    ".COM".to_string(),
                    ".EXE".to_string(),
                    ".BAT".to_string(),
                    ".CMD".to_string(),
                ]
            }
            #[cfg(not(windows))]
            {
                Vec::new()
            }
        });

    std::env::split_paths(&path).all(|directory| {
        let directory = directory.as_path();
        if directory.join(tool).is_file() {
            return false;
        }
        !extensions.iter().any(|extension| {
            let candidate = directory.join(format!("{tool}{extension}"));
            if candidate.is_file() {
                return true;
            }
            // Windows PATHEXT matching is case-insensitive (a `.bat`
            // shim satisfies `.BAT`) — honor that on case-sensitive
            // filesystems too, or Linux checkouts miss what Windows sees.
            // (PATHEXT Windows không phân biệt hoa thường.)
            let want = format!("{tool}{extension}").to_lowercase();
            std::fs::read_dir(directory)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.file_name().to_string_lossy().to_lowercase() == want)
        })
    })
}

/// Resolve the Python launcher name for this machine (P0 fix,
/// 2026-09-12): macOS ships only `python3` (no `python` shim since
/// Monterey) while setup-python CI images provide `python`. The
/// honest lifecycle matrix caught the hardcoded `python` failing on
/// macOS — build/install lanes must resolve the launcher that EXISTS,
/// never guess one.
/// Chọn tên launcher Python của máy này (P0 fix, 2026-09-12): macOS chỉ
/// có `python3` (không còn shim `python` từ Monterey) còn image CI
/// setup-python có `python`. Matrix lifecycle trung thực đã bắt được
/// hardcoded `python` fail trên macOS — lane build/install phải chọn
/// launcher TỒN TẠI, không đoán.
pub(crate) fn python_cmd() -> &'static str {
    if !tool_unavailable("python") {
        "python"
    } else if !tool_unavailable("python3") {
        "python3"
    } else {
        "python" // Neither exists — surface the honest toolchain error.
    }
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

// P1 split (2026-09-11): the web build engine (target resolution,
// framework script map, node bin shims, native engine build) lives in
// build/web_engine.rs — build.rs keeps the per-core dispatch only.
mod web_engine;
use crate::context::ProjectContext;

// Test-only re-export: build_test.rs consumes engine symbols via
// `use super::*` (RULE §5 module-test pattern).
#[cfg(test)]
pub(crate) use web_engine::*;
use web_engine::{
    build_rust_with_env, build_web, detect_app_runtime, detect_lib_runtime, node_bin_args,
    prepend_path, run_allowlisted_tool, run_allowlisted_tool_with_env,
};

#[cfg(test)]
#[path = "../test/build_test.rs"]
mod tests;
