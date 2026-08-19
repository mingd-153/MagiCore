use anyhow::Result;
use std::path::{Path, PathBuf};

fn not_available(reason: &str) -> anyhow::Error {
    crate::error::app_not_available(reason)
}

pub fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    mg_app_adapter::adapter_for(&cwd)
        .map(|_| cwd.clone())
        .ok_or_else(crate::error::app_project_not_detected)
}

pub fn language(root: &Path) -> Result<mg_app_adapter::AppLanguage> {
    mg_app_adapter::adapter_for(root)
        .map(|a| a.language)
        .ok_or_else(|| crate::error::no_app_language(root))
}

/// Lệnh install theo language — Q18 (allowlist §5.1: flutter/pub/gradle/swift).
pub struct InstallCommand {
    pub tool: String,
    pub args: Vec<String>,
}

fn install_command(lang: mg_app_adapter::AppLanguage) -> InstallCommand {
    match lang {
        mg_app_adapter::AppLanguage::Flutter => InstallCommand {
            tool: "flutter".to_string(),
            args: vec!["pub".to_string(), "get".to_string()],
        },
        mg_app_adapter::AppLanguage::Kotlin => InstallCommand {
            tool: "gradle".to_string(),
            args: vec!["dependencies".to_string()],
        },
        mg_app_adapter::AppLanguage::Swift => InstallCommand {
            tool: "swift".to_string(),
            args: vec!["package".to_string(), "resolve".to_string()],
        },
        // react-native: npm trong subdir RN — C9 scoped exception (npm policy §5.1)
        mg_app_adapter::AppLanguage::ReactNative => InstallCommand {
            tool: "npm".to_string(),
            args: vec!["install".to_string()],
        },
        mg_app_adapter::AppLanguage::ObjC => InstallCommand {
            tool: String::new(),
            args: vec![],
        },
        mg_app_adapter::AppLanguage::Multi => InstallCommand {
            tool: String::new(),
            args: vec![],
        },
    }
}

/// Lệnh dev theo language — Q20 (flutter run / gradle run / swift run).
fn dev_command(lang: mg_app_adapter::AppLanguage) -> InstallCommand {
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

pub fn run_tool(root: &Path, cmd: &str, args: &[String]) -> Result<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run_inherited(cmd, args, &opts)
        .map_err(|e| crate::error::app_tool_failed(cmd, &e))?;
    Ok(())
}

/// Lệnh theo language cho verb — None = không có CLI passthrough, sửa manifest tay.
pub fn tool_command(lang: mg_app_adapter::AppLanguage, verb: &str) -> Option<InstallCommand> {
    let (tool, base): (&str, &[&str]) = match (lang, verb) {
        (mg_app_adapter::AppLanguage::Flutter, "add") => ("flutter", &["pub", "add"]),
        (mg_app_adapter::AppLanguage::Flutter, "remove") => ("flutter", &["pub", "remove"]),
        (mg_app_adapter::AppLanguage::Flutter, "list") => ("flutter", &["pub", "deps"]),
        (mg_app_adapter::AppLanguage::Flutter, "update") => ("flutter", &["pub", "upgrade"]),
        (mg_app_adapter::AppLanguage::Kotlin, "list") => ("gradle", &["dependencies"]),
        (mg_app_adapter::AppLanguage::Swift, "list") => {
            ("swift", &["package", "show-dependencies"])
        }
        _ => return None,
    };
    Some(InstallCommand {
        tool: tool.to_string(),
        args: base.iter().map(|s| s.to_string()).collect(),
    })
}

pub fn manifest_hint(lang: mg_app_adapter::AppLanguage, verb: &str) -> anyhow::Error {
    let file = match lang {
        mg_app_adapter::AppLanguage::Flutter => "pubspec.yaml",
        mg_app_adapter::AppLanguage::Kotlin => "android/app/build.gradle(.kts)",
        mg_app_adapter::AppLanguage::Swift => "Package.swift",
        mg_app_adapter::AppLanguage::ReactNative => "package.json (npm — C9 scoped exception)",
        mg_app_adapter::AppLanguage::ObjC => "iOS Podfile",
        mg_app_adapter::AppLanguage::Multi => "platform subproject manifest",
    };
    crate::error::manifest_hint(verb, &lang, file)
}

/// `mg install` — passthrough tool theo language; `--dry-run` in lệnh không chạy.
pub async fn install(packages: Vec<String>, dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;

    if !packages.is_empty() && dry_run {
        mg_ui::info("[dry-run] ignoring package args — app deps flow through provider tooling");
    }

    if lang == mg_app_adapter::AppLanguage::Multi {
        return install_multi(&root, dry_run).await;
    }
    if matches!(lang, mg_app_adapter::AppLanguage::ObjC) {
        return install_objc(&root, dry_run).await;
    }

    let cmd = install_command(lang);
    if cmd.tool.is_empty() {
        return Err(not_available(
            "has no install flow for this language yet — edit manifest and resolve with the platform tool",
        ));
    }
    if dry_run {
        mg_ui::info(&format!(
            "[dry-run] would run: {} {} (real install runs when the tool is present — drop `--dry-run`)",
            cmd.tool,
            cmd.args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!("Installing: {} {}", cmd.tool, cmd.args.join(" ")));
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}

/// Multi: install từng platform trong subdir; toolchain thiếu → skip cảnh báo.
async fn install_multi(root: &Path, dry_run: bool) -> Result<()> {
    let (android, ios, flutter) = platform_install_commands();
    let mut platforms: Vec<(&str, PathBuf, InstallCommand)> = vec![
        ("android", root.join("android"), android),
        ("ios", root.join("ios"), ios),
        ("flutter", root.join("flutter"), flutter),
    ];
    let rn = InstallCommand {
        tool: "npm".to_string(),
        args: vec!["install".to_string()],
    };
    platforms.push(("react-native", root.join("react-native"), rn));
    for (name, dir, cmd) in platforms {
        if !dir.exists() {
            mg_ui::info(&format!("Platform '{name}' missing directory — skipping"));
            continue;
        }
        if cmd.tool.is_empty() {
            continue;
        }
        if tool_unavailable(&cmd.tool) {
            mg_ui::warning(&format!("{} not found — skipping {name} install", cmd.tool));
            continue;
        }
        if dry_run {
            mg_ui::info(&format!(
                "[dry-run] would run in {name}/: {} {}",
                cmd.tool,
                cmd.args.join(" ")
            ));
            continue;
        }
        mg_ui::info(&format!(
            "Installing {name}: {} {}",
            cmd.tool,
            cmd.args.join(" ")
        ));
        run_tool(&dir, &cmd.tool, &cmd.args)?;
    }
    Ok(())
}

fn tool_unavailable(tool: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|dir| std::path::Path::new(dir).join(tool))
        .find(|p| p.is_file())
        .is_none()
}

/// react-native (npm, C9 scoped exception) — shared (KMP gradle) chạy qua android gradle build.
fn platform_install_commands() -> (InstallCommand, InstallCommand, InstallCommand) {
    (
        InstallCommand {
            tool: "gradle".to_string(),
            args: vec!["dependencies".to_string()],
        },
        InstallCommand {
            tool: "swift".to_string(),
            args: vec!["package".to_string(), "resolve".to_string()],
        },
        InstallCommand {
            tool: "flutter".to_string(),
            args: vec!["pub".to_string(), "get".to_string()],
        },
    )
}

/// objC: resolve package dependencies qua xcodebuild (allowlist §3 — xcodebuild P2).
/// Project/workspace tìm từ thư mục con; không thấy → lỗi rõ.
async fn install_objc(root: &Path, dry_run: bool) -> Result<()> {
    let Some(xcode_project) = find_xcode_project(root) else {
        return Err(crate::error::xcode_project_missing(root));
    };
    let (flag, name) = if xcode_project.ends_with(".xcworkspace") {
        ("-workspace", xcode_project.as_str())
    } else {
        ("-project", xcode_project.as_str())
    };
    let args: Vec<String> = vec![
        "-resolvePackageDependencies".to_string(),
        flag.to_string(),
        name.to_string(),
    ];
    if dry_run {
        mg_ui::info(&format!(
            "[dry-run] would run: xcodebuild {} (real install runs when Xcode is present)",
            args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!("Installing objC: xcodebuild {}", args.join(" ")));
    run_tool(root, "xcodebuild", &args)
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

/// [app] dev_scheme trong mg.toml — objC dev chạy xcodebuild build; thiếu → dùng Xcode IDE.
pub fn dev_scheme(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("mg.toml")).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("app")
        .and_then(|a| a.get("dev_scheme"))
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// objC dev — có [app] dev_scheme → xcodebuild build (simulator), không → mở Xcode.
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
        mg_ui::info(&format!(
            "[dry-run] would run: xcodebuild {}",
            args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!("App dev (objC): xcodebuild {}", args.join(" ")));
    run_tool(root, "xcodebuild", &args)
}

#[cfg(test)]
#[path = "test/app.rs"]
mod tests;
