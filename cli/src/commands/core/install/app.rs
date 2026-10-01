use anyhow::Result;
use std::path::Path;

pub fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    mgc_app_adapter::adapter_for(&cwd).ok_or_else(crate::error::app_project_not_detected)?;
    crate::commands::core::shared::ensure_project_core_identity(&cwd, "app")?;
    Ok(cwd)
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

/// Report the manifest and operation needed to implement this app dependency verb.
/// Báo manifest và thao tác cần thiết để hỗ trợ lệnh dependency app này.
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

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
