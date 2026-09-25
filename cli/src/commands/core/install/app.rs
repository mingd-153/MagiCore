use anyhow::Result;
use std::path::Path;

pub fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    mgc_app_adapter::adapter_for(&cwd)
        .map(|_| cwd.clone())
        .ok_or_else(crate::error::app_project_not_detected)
}

pub fn language(root: &Path) -> Result<mgc_app_adapter::AppLanguage> {
    mgc_app_adapter::adapter_for(root)
        .map(|a| a.language)
        .ok_or_else(|| crate::error::no_app_language(root))
}

/// React Native has NO runner for any verb: run the gate FIRST so the
/// app/rn Unsupported rule owns the failure (P0#2) instead of a manifest
/// hint. Every other language returns Ok and continues to verb
/// resolution (unimplemented verbs keep their honest manifest hints —
/// the gate must not promise a compat opt-in for a verb that has no
/// command at all).
/// (RN không có runner cho verb nào: gate TRƯỚC để rule Unsupported sở
/// hữu lỗi.)
pub fn gate_react_native(
    root: &Path,
    lang: mgc_app_adapter::AppLanguage,
    op: crate::commands::dep_gate::DepOp,
    compat: &crate::commands::compat::CompatMode,
) -> Result<()> {
    if lang == mgc_app_adapter::AppLanguage::ReactNative {
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "app",
                Some(lang.ecosystem()),
                None,
                None,
                op,
            ),
            None,
            compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
    }
    Ok(())
}

/// Provider-toolchain install command for lanes that still explicitly delegate.
/// Lệnh cài đặt dành cho lane còn được phép delegate sang toolchain.
pub struct InstallCommand {
    pub tool: String,
    pub args: Vec<String>,
}

/// Native flutter install: pubspec → pub.dev → mgc.lock, no toolchain.
/// (Install flutter native, không spawn toolchain.)
pub async fn install_flutter_native(
    root: &std::path::Path,
    packages: Vec<String>,
    dry_run: bool,
    compat_runtime: Option<String>,
    frozen: bool,
) -> Result<()> {
    if !packages.is_empty() {
        return Err(crate::error::install_app_packages_use_add(&packages));
    }
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    let adapter =
        mgc_app_adapter::adapter_for(root).ok_or_else(crate::error::app_project_not_detected)?;
    crate::commands::dep_gate::gate_native_adapter(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(mgc_app_adapter::AppLanguage::Flutter.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::Install,
        ),
        &adapter,
        &compat,
    )?;
    if dry_run {
        mgc_ui::info(
            "[dry-run] native Flutter dependency install is available; no package manager will be spawned",
        );
        return Ok(());
    }
    let adapter: std::sync::Arc<dyn mgc_types::adapter::PackageAdapter> =
        std::sync::Arc::new(adapter);
    crate::commands::core::shared::install_with_adapter(
        &*adapter,
        root,
        "mgc add",
        frozen,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}

/// Lệnh dev theo language — Q20 (flutter run / gradle run / swift run).
///
/// DELEGATED: these commands run the native toolchains for real (mgc dev
/// passthrough) — mgc does not own their lifecycles.
/// (DELEGATED: các lệnh chạy toolchain gốc thật (passthrough mgc dev) —
/// mgc không sở hữu lifecycle của chúng.)
#[allow(dead_code)]
fn dev_command(lang: mgc_app_adapter::AppLanguage) -> InstallCommand {
    match lang {
        mgc_app_adapter::AppLanguage::Flutter => InstallCommand {
            tool: "flutter".to_string(),
            args: vec!["run".to_string()],
        },
        mgc_app_adapter::AppLanguage::Kotlin => InstallCommand {
            tool: "gradle".to_string(),
            args: vec!["run".to_string()],
        },
        mgc_app_adapter::AppLanguage::Swift => InstallCommand {
            tool: "swift".to_string(),
            args: vec!["run".to_string()],
        },
        mgc_app_adapter::AppLanguage::ReactNative => InstallCommand {
            tool: String::new(),
            args: vec![],
        },
        mgc_app_adapter::AppLanguage::ObjC | mgc_app_adapter::AppLanguage::Multi => {
            InstallCommand {
                tool: String::new(),
                args: vec![],
            }
        }
    }
}

pub fn run_tool(root: &Path, cmd: &str, args: &[String]) -> Result<()> {
    run_tool_with_env(root, cmd, args, None)
}

/// Run tool with optional env vars from optimizer
/// Chạy tool với env vars tùy chọn từ optimizer
pub fn run_tool_with_env(
    root: &Path,
    cmd: &str,
    args: &[String],
    env: Option<Vec<(String, String)>>,
) -> Result<()> {
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        env: env.unwrap_or_default(),
        clean_env: false, // Preserve env when custom env provided
        ..Default::default()
    };
    mgc_exec::prelude::run_inherited(cmd, args, &opts)
        .map_err(|e| crate::error::app_tool_failed(cmd, &e))?;
    Ok(())
}

/// Dependency verbs never fall back to provider package managers.
/// Lệnh dependency không bao giờ fallback sang package manager bên ngoài.
pub fn tool_command(lang: mgc_app_adapter::AppLanguage, verb: &str) -> Option<InstallCommand> {
    let _ = (lang, verb);
    None
}

pub fn manifest_hint(lang: mgc_app_adapter::AppLanguage, verb: &str) -> anyhow::Error {
    let file = match lang {
        mgc_app_adapter::AppLanguage::Flutter => "pubspec.yaml",
        mgc_app_adapter::AppLanguage::Kotlin => "android/app/build.gradle(.kts)",
        mgc_app_adapter::AppLanguage::Swift => "Package.swift",
        mgc_app_adapter::AppLanguage::ReactNative => {
            "package.json (MagiCore-native runner pending)"
        }
        mgc_app_adapter::AppLanguage::ObjC => "iOS Podfile",
        mgc_app_adapter::AppLanguage::Multi => "platform subproject manifest",
    };
    crate::error::manifest_hint(verb, &format!("{lang:?}"), file)
}

/// `mgc install app` only enters an implemented MagiCore-native lane.
pub async fn install(
    packages: Vec<String>,
    dry_run: bool,
    compat_runtime: Option<String>,
    frozen: bool,
) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;

    if matches!(
        lang,
        mgc_app_adapter::AppLanguage::Flutter
            | mgc_app_adapter::AppLanguage::Swift
            | mgc_app_adapter::AppLanguage::ReactNative
    ) {
        return install_app_native(&root, lang, packages, dry_run, compat_runtime, frozen).await;
    }

    Err(crate::error::native_dependency_engine_unavailable(
        "app",
        lang.ecosystem(),
        "install",
    ))
}

async fn install_app_native(
    root: &Path,
    lang: mgc_app_adapter::AppLanguage,
    packages: Vec<String>,
    dry_run: bool,
    compat_runtime: Option<String>,
    frozen: bool,
) -> Result<()> {
    if !packages.is_empty() && lang != mgc_app_adapter::AppLanguage::Flutter {
        return Err(crate::error::native_dependency_engine_unavailable(
            "app",
            lang.ecosystem(),
            "install package arguments; use the native add operation",
        ));
    }
    if lang == mgc_app_adapter::AppLanguage::Flutter {
        return install_flutter_native(root, packages, dry_run, compat_runtime, frozen).await;
    }
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    let adapter =
        mgc_app_adapter::adapter_for(root).ok_or_else(crate::error::app_project_not_detected)?;
    crate::commands::dep_gate::gate_native_adapter(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(lang.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::Install,
        ),
        &adapter,
        &compat,
    )?;
    if dry_run {
        mgc_ui::info(&format!(
            "[dry-run] native app dependency install is available for {}; no package manager will be spawned",
            lang.as_str()
        ));
        return Ok(());
    }
    crate::commands::core::shared::install_with_adapter(
        &adapter,
        root,
        "mgc install app",
        frozen,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}

/// Tìm Xcode project — ưu tiên workspace, fallback project, không đệ quy sâu.
pub fn find_xcode_project(root: &Path) -> Option<String> {
    let mut workspace: Option<String> = None;
    let mut project: Option<String> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".xcworkspace") && workspace.is_none() {
            workspace = Some(name);
        } else if name.ends_with(".xcodeproj") && project.is_none() {
            project = Some(name);
        }
    }
    workspace.or(project)
}

/// [app] dev_scheme trong mgc.toml — objC dev chạy xcodebuild build; thiếu → dùng Xcode IDE.
pub fn dev_scheme(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("mgc.toml")).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("app")
        .and_then(|a| a.get("dev_scheme"))
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// objC dev — có [app] dev_scheme → xcodebuild build (simulator), không → mở Xcode.
#[allow(dead_code)]
async fn dev_objc(root: &Path, dry_run: bool) -> Result<()> {
    let Some(scheme) = dev_scheme(root) else {
        let Some(proj) = find_xcode_project(root) else {
            return Err(crate::error::xcode_project_missing_short());
        };
        return Err(crate::error::objc_dev_needs_xcode(&proj));
    };
    let args: Vec<String> = vec![
        "-scheme".to_string(),
        scheme,
        "-destination".to_string(),
        "platform=iOS Simulator,name=iPhone 16".to_string(),
        "build".to_string(),
    ];
    if dry_run {
        mgc_ui::info(&format!(
            "[dry-run] would run: xcodebuild {}",
            args.join(" ")
        ));
        return Ok(());
    }
    mgc_ui::info(&format!("App dev (objC): xcodebuild {}", args.join(" ")));
    run_tool(root, "xcodebuild", &args)
}

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
