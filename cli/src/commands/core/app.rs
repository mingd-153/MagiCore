use anyhow::Result;
use std::path::{Path, PathBuf};

fn not_available(reason: &str) -> anyhow::Error {
    anyhow::anyhow!("'app' {reason}")
}

fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    mg_app_adapter::adapter_for(&cwd)
        .map(|_| cwd.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot detect an app project here (missing mg.toml [app] language / pubspec.yaml / build.gradle/.kts / Package.swift)."
            )
        })
}

fn language(root: &Path) -> Result<mg_app_adapter::AppLanguage> {
    mg_app_adapter::adapter_for(root)
        .map(|a| a.language)
        .ok_or_else(|| anyhow::anyhow!("No app language detected in {}", root.display()))
}

/// Lệnh install theo language — Q18 (allowlist §5.1: flutter/pub/gradle/swift).
struct InstallCommand {
    tool: String,
    args: Vec<String>,
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

fn run_tool(root: &Path, cmd: &str, args: &[String]) -> Result<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run_inherited(cmd, args, &opts).map_err(|e| {
        anyhow::anyhow!(
            "{} failed: {e} — install the tool first and ensure it is in PATH",
            cmd
        )
    })?;
    Ok(())
}

/// Lệnh theo language cho verb — None = không có CLI passthrough, sửa manifest tay.
fn tool_command(lang: mg_app_adapter::AppLanguage, verb: &str) -> Option<InstallCommand> {
    let (tool, base): (&str, &[&str]) = match (lang, verb) {
        (mg_app_adapter::AppLanguage::Flutter, "add") => ("flutter", &["pub", "add"]),
        (mg_app_adapter::AppLanguage::Flutter, "remove") => ("flutter", &["pub", "remove"]),
        (mg_app_adapter::AppLanguage::Flutter, "list") => ("flutter", &["pub", "deps"]),
        (mg_app_adapter::AppLanguage::Flutter, "update") => {
            ("flutter", &["pub", "upgrade"])
        }
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

fn manifest_hint(lang: mg_app_adapter::AppLanguage, verb: &str) -> anyhow::Error {
    let file = match lang {
        mg_app_adapter::AppLanguage::Flutter => "pubspec.yaml",
        mg_app_adapter::AppLanguage::Kotlin => "android/app/build.gradle(.kts)",
        mg_app_adapter::AppLanguage::Swift => "Package.swift",
        mg_app_adapter::AppLanguage::ReactNative => "package.json (npm — C9 scoped exception)",
        mg_app_adapter::AppLanguage::ObjC => "iOS Podfile",
        mg_app_adapter::AppLanguage::Multi => "platform subproject manifest",
    };
    anyhow::anyhow!(
        "'{verb}' for {lang:?} has no CLI passthrough — edit {file} then run `mg install`."
    )
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
            "[dry-run] would run: {} {} (install chạy thật khi có tool — bỏ `--dry-run`)",
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
        anyhow::bail!(
            "không tìm thấy *.xcworkspace/*.xcodeproj dưới {} — objC app thiếu Xcode project",
            root.display()
        );
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
            "[dry-run] would run: xcodebuild {} (install chạy thật khi có Xcode)",
            args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!(
        "Installing objC: xcodebuild {}",
        args.join(" ")
    ));
    run_tool(root, "xcodebuild", &args)
}

/// Tìm Xcode project — ưu tiên workspace, fallback project, không đệ quy sâu.
fn find_xcode_project(root: &Path) -> Option<String> {
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
fn dev_scheme(root: &Path) -> Option<String> {
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
            anyhow::bail!("không tìm thấy *.xcworkspace/*.xcodeproj — objC app thiếu Xcode project");
        };
        anyhow::bail!(
            "objC dev chạy trong Xcode — mở {proj}, hoặc khai [app] dev_scheme = \"<scheme>\" trong mg.toml để `mg dev` chạy xcodebuild build trên simulator"
        );
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
            anyhow::bail!(
                "multi dev P1 targets flutter/ entry — run `mg build` for other platforms"
            );
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
    if lang == mg_app_adapter::AppLanguage::Multi {
        anyhow::bail!(
            "đa platform dev P1 chạy qua từng subdir: `mg dev` ở flutter/ hoặc react-native/ (xem `mg build`)"
        );
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

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    _version: Option<String>,
    _dev: bool,
    _exact: bool,
    _optional: bool,
    _peer: bool,
    _no_save: bool,
    _global: bool,
) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    if packages.is_empty() {
        anyhow::bail!("mg add-app <pkg> [pkg...] — khai tên package cần thêm");
    }
    let Some(mut cmd) = tool_command(lang, "add") else {
        return Err(manifest_hint(lang, "add"));
    };
    cmd.args.extend(packages.iter().flat_map(|p| p.split_whitespace().map(String::from)));
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    if packages.is_empty() {
        anyhow::bail!("mg remove-app <pkg> [pkg...] — khai tên package cần gỡ");
    }
    let Some(mut cmd) = tool_command(lang, "remove") else {
        return Err(manifest_hint(lang, "remove"));
    };
    cmd.args.extend(packages.iter().flat_map(|p| p.split_whitespace().map(String::from)));
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    let Some(cmd) = tool_command(lang, "list") else {
        return Err(manifest_hint(lang, "list"));
    };
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}

pub async fn update(packages: Vec<String>, _install: bool) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    let Some(mut cmd) = tool_command(lang, "update") else {
        return Err(manifest_hint(lang, "update"));
    };
    cmd.args.extend(packages.iter().flat_map(|p| p.split_whitespace().map(String::from)));
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::app::AppWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        if let Some(fw) = config.frameworks.first() {
            // Registry-first: fetch layer app/<fw> nếu chưa có; fetch fail → fallback procedural.
            crate::commands::template::ensure_layer(&format!("app/{fw}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success("App project created. Run `mg install` or `mg dev` next.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_command_per_language() {
        let fl = install_command(mg_app_adapter::AppLanguage::Flutter);
        assert_eq!(fl.tool, "flutter");
        assert_eq!(fl.args, vec!["pub", "get"]);
        let kt = install_command(mg_app_adapter::AppLanguage::Kotlin);
        assert_eq!(kt.tool, "gradle");
        assert_eq!(kt.args, vec!["dependencies"]);
        let sw = install_command(mg_app_adapter::AppLanguage::Swift);
        assert_eq!(sw.tool, "swift");
        assert_eq!(sw.args, vec!["package", "resolve"]);
        let rn = install_command(mg_app_adapter::AppLanguage::ReactNative);
        assert_eq!(rn.tool, "npm");
        assert_eq!(rn.args, vec!["install"]);
    }

    #[test]
    fn dev_command_per_language() {
        assert_eq!(
            dev_command(mg_app_adapter::AppLanguage::Flutter).args,
            vec!["run"]
        );
        assert_eq!(
            dev_command(mg_app_adapter::AppLanguage::Kotlin).tool,
            "gradle"
        );
        assert_eq!(
            dev_command(mg_app_adapter::AppLanguage::Swift).args,
            vec!["run"]
        );
        assert_eq!(
            dev_command(mg_app_adapter::AppLanguage::ReactNative).args,
            vec!["run", "android"]
        );
    }

    #[test]
    fn xcode_project_prefers_workspace() {
        let dir = std::env::temp_dir().join(format!("mg-app-xc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("App.xcodeproj"), "").unwrap();
        std::fs::write(dir.join("App.xcworkspace"), "").unwrap();
        assert_eq!(find_xcode_project(&dir).unwrap(), "App.xcworkspace");
        std::fs::remove_file(dir.join("App.xcworkspace")).unwrap();
        assert_eq!(find_xcode_project(&dir).unwrap(), "App.xcodeproj");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dev_scheme_reads_mg_toml() {
        let dir = std::env::temp_dir().join(format!("mg-app-scheme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(dev_scheme(&dir).is_none());
        std::fs::write(
            dir.join("mg.toml"),
            "[app]\nlanguage = \"objc\"\ndev_scheme = \"App\"\n",
        )
        .unwrap();
        assert_eq!(dev_scheme(&dir).unwrap(), "App");
        std::fs::write(dir.join("mg.toml"), "[app]\nlanguage = \"objc\"\n").unwrap();
        assert!(dev_scheme(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
