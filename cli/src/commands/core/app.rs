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
        mg_app_adapter::AppLanguage::ReactNative | mg_app_adapter::AppLanguage::ObjC => {
            InstallCommand {
                tool: String::new(),
                args: vec![],
            }
        }
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
        mg_app_adapter::AppLanguage::ReactNative
        | mg_app_adapter::AppLanguage::ObjC
        | mg_app_adapter::AppLanguage::Multi => InstallCommand {
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
    if matches!(
        lang,
        mg_app_adapter::AppLanguage::ReactNative | mg_app_adapter::AppLanguage::ObjC
    ) {
        return Err(not_available(
            "has no install flow yet — react-native/objc resolution is a future track (§5.2 npm policy)",
        ));
    }

    let cmd = install_command(lang);
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
    if matches!(
        lang,
        mg_app_adapter::AppLanguage::ReactNative | mg_app_adapter::AppLanguage::ObjC
    ) {
        return Err(not_available(
            "has no dev flow yet — react-native/objc targets are scaffold-only in P1",
        ));
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
    let _ = packages;
    Err(not_available(
        "has no package manager — add deps with provider tooling (flutter pub add / gradle / swift package add).",
    ))
}
pub async fn remove(_packages: Vec<String>) -> Result<()> {
    Err(not_available(
        "has no package manager — remove via provider tooling.",
    ))
}
pub async fn list() -> Result<()> {
    Err(not_available(
        "has no package registry — inspect deps with provider tooling (pub deps / gradle dependencies / swift package show-dependencies).",
    ))
}
pub async fn update(_packages: Vec<String>, _install: bool) -> Result<()> {
    Err(not_available(
        "dep updates flow through provider tooling (flutter pub upgrade / gradle / swift package update).",
    ))
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
    }
}
