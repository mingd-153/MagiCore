//! `mg dev` app — chạy app qua tool theo language (Q20). Helpers từ install/app.rs.

use anyhow::Result;

use crate::commands::core::install::app::{
    find_xcode_project, language, project_root, run_tool, InstallCommand,
};

/// Lệnh dev theo language — Q20 (flutter run / gradle run / swift run).
pub fn dev_command(lang: mg_app_adapter::AppLanguage) -> InstallCommand {
    match lang {
        mg_app_adapter::AppLanguage::Flutter => InstallCommand {
            tool: "flutter".to_string(),
            args: vec!["run".to_string()],
        },
        mg_app_adapter::AppLanguage::Kotlin => InstallCommand {
            tool: "gradle".to_string(),
            args: vec!["run".to_string()],
        },
        mg_app_adapter::AppLanguage::Swift => InstallCommand {
            tool: "swift".to_string(),
            args: vec!["run".to_string()],
        },
        mg_app_adapter::AppLanguage::ReactNative => InstallCommand {
            tool: "npm".to_string(),
            args: vec!["run".to_string(), "android".to_string()],
        },
        mg_app_adapter::AppLanguage::ObjC | mg_app_adapter::AppLanguage::Multi => InstallCommand {
            tool: String::new(),
            args: vec![],
        },
    }
}

/// [app] dev_scheme trong mg.toml — objC dev chạy xcodebuild build; thiếu → dùng Xcode IDE.
fn dev_scheme(root: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("mg.toml")).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("app")
        .and_then(|a| a.get("dev_scheme"))
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// objC dev — có [app] dev_scheme → xcodebuild build (simulator), không → mở Xcode.
async fn dev_objc(root: &std::path::Path, dry_run: bool) -> Result<()> {
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
        mg_ui::info(&format!(
            "[dry-run] would run: xcodebuild {}",
            args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!("App dev (objC): xcodebuild {}", args.join(" ")));
    run_tool(root, "xcodebuild", &args)
}

/// `mg dev` — chạy app qua tool theo language.
pub async fn dev(_dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    if lang == mg_app_adapter::AppLanguage::Multi {
        let dir = root.join("flutter");
        if !dir.exists() {
            return Err(crate::error::multi_dev_flutter_only());
        }
        let cmd = InstallCommand {
            tool: "flutter".to_string(),
            args: vec!["run".to_string()],
        };
        mg_ui::info(&format!(
            "App dev (flutter entry): running `{} {}`...",
            cmd.tool,
            cmd.args.join(" ")
        ));
        return run_tool(&dir, &cmd.tool, &cmd.args);
    }
    if lang == mg_app_adapter::AppLanguage::ObjC {
        return dev_objc(&root, _dry_run).await;
    }
    let cmd = dev_command(lang);
    mg_ui::info(&format!(
        "App dev: running `{} {}`...",
        cmd.tool,
        cmd.args.join(" ")
    ));
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
