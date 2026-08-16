use anyhow::Result;
use std::path::Path;

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
    let cmd = install_command(lang);

    if !packages.is_empty() && dry_run {
        mg_ui::info("[dry-run] ignoring package args — app deps flow through provider tooling");
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

/// `mg dev` — chạy app qua tool theo language.
pub async fn dev(_dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
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
