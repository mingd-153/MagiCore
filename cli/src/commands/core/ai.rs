use anyhow::Result;
use std::path::PathBuf;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    mg_ai_adapter::adapter_for(&cwd)
        .map(|_| cwd.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot detect an ai project here (missing mg.toml [ai] framework / pyproject [tool.megagate] framework)."
            )
        })
}

/// `mg dev` ai — chạy entry script qua python3 (Q20, allowlist §5.1).
pub async fn dev(_dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let framework = mg_ai_adapter::adapter_for(&root)
        .ok_or_else(|| anyhow::anyhow!("No ai framework detected in {}", root.display()))?;
    let script = framework.framework.entry_script().to_string();

    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.clone()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    let cmd = "python3".to_string();
    mg_ui::info(&format!("AI dev: running `{} {}`...", cmd, script));
    mg_exec::prelude::run_inherited(&cmd, &[script], &opts).map_err(|e| {
        anyhow::anyhow!(
            "python3 failed: {e} — install Python 3.11+ and ensure `python3` is in PATH"
        )
    })?;
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
    // 05 §5: chốt 1 tool theo lock hiện có — ưu tiên uv, fallback pip (Q9 exec passthrough)
    let tool = pick_ai_tool(&root);
    let mut args = vec![if tool == "uv" {
        "add".to_string()
    } else {
        "install".to_string()
    }];
    args.extend(packages.iter().flat_map(|p| p.split_whitespace().map(String::from)));
    run_ai_tool(&root, tool, &args)?;
    Ok(())
}

/// Chọn tool theo lock: uv.lock → uv, requirements.lock → pip, mặc định uv nếu có, else pip.
fn pick_ai_tool(root: &std::path::Path) -> &'static str {
    if root.join("uv.lock").exists() {
        "uv"
    } else if root.join("requirements.lock").exists() {
        "pip"
    } else if tool_uv_available() {
        "uv"
    } else {
        "pip"
    }
}

fn tool_uv_available() -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|dir| PathBuf::from(dir).join("uv"))
        .any(|p| p.is_file())
}

fn run_ai_tool(root: &std::path::Path, tool: &str, args: &[String]) -> Result<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run_inherited(tool, args, &opts)
        .map_err(|e| anyhow::anyhow!("{tool} failed: {e}"))?;
    Ok(())
}

fn run_ai_tool_capture(
    root: &std::path::Path,
    tool: &str,
    args: &[String],
) -> Result<String> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    let report = mg_exec::prelude::run(tool, args, &opts)
        .map_err(|e| anyhow::anyhow!("{tool} failed: {e}"))?;
    Ok(report.stdout_tail)
}

pub async fn remove(packages: Vec<String>) -> Result<()> {
    if packages.is_empty() {
        anyhow::bail!("mg remove-ai <pkg> [pkg...] — khai tên package cần gỡ");
    }
    let root = project_root()?;
    let tool = pick_ai_tool(&root);
    let mut args = vec![if tool == "uv" {
        "remove".to_string()
    } else {
        "uninstall".to_string()
    }];
    if tool == "pip" {
        args.push("-y".to_string());
    }
    args.extend(packages.iter().flat_map(|p| p.split_whitespace().map(String::from)));
    run_ai_tool(&root, tool, &args)?;
    Ok(())
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let tool = pick_ai_tool(&root);
    let args = if tool == "uv" {
        vec!["pip".to_string(), "list".to_string()]
    } else {
        vec!["list".to_string()]
    };
    run_ai_tool(&root, tool, &args)?;
    Ok(())
}

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = project_root()?;
    let tool = pick_ai_tool(&root);
    if packages.is_empty() {
        // Không có tên package: cập nhật lock (uv) hoặc báo outdated (pip).
        if tool == "uv" {
            run_ai_tool(&root, tool, &["lock".to_string(), "--upgrade".to_string()])?;
            if install {
                run_ai_tool(&root, tool, &["sync".to_string()])?;
            }
        } else {
            mg_ui::info("Chạy `pip list --outdated` để xem bản mới — không tự upgrade lock bằng pip.");
            run_ai_tool(&root, tool, &["list".to_string(), "--outdated".to_string()])?;
        }
        return Ok(());
    }
    let mut args = if tool == "uv" {
        vec!["lock".to_string(), "--upgrade-package".to_string()]
    } else {
        vec!["install".to_string(), "--upgrade".to_string()]
    };
    args.extend(packages.iter().flat_map(|p| p.split_whitespace().map(String::from)));
    if tool == "uv" {
        args = vec!["lock".to_string()];
        for p in packages.iter().flat_map(|p| p.split_whitespace()) {
            args.push("--upgrade-package".to_string());
            args.push(p.to_string());
        }
    }
    run_ai_tool(&root, tool, &args)?;
    if tool == "uv" {
        if install {
            run_ai_tool(&root, tool, &["sync".to_string()])?;
        }
    } else {
        // pip: cập nhật lock sau khi upgrade
        let locked = run_ai_tool_capture(&root, tool, &["freeze".to_string()])?;
        std::fs::write(root.join("requirements.lock"), locked)?;
    }
    Ok(())
}
pub async fn install(packages: Vec<String>, dry_run: bool) -> Result<()> {
    let root = project_root()?;
    if !packages.is_empty() {
        mg_ui::info(&format!(
            "[ai install] ignoring package args {:?} — install theo lock (05 §5)",
            packages
        ));
    }
    // Lock ghép: ưu tiên uv.lock (deterministic), fallback requirements.lock (pip freeze)
    let (tool, args): (&str, Vec<String>) = if root.join("uv.lock").exists() {
        ("uv", vec!["sync".to_string()])
    } else if root.join("requirements.lock").exists() {
        (
            "pip",
            vec![
                "install".to_string(),
                "-r".to_string(),
                "requirements.lock".to_string(),
            ],
        )
    } else {
        anyhow::bail!(
            "không có lock file (uv.lock hoặc requirements.lock) — chạy `mg add <pkg>` để tạo lock trước"
        );
    };
    if dry_run {
        mg_ui::info(&format!("[dry-run] {} {}", tool, args.join(" ")));
        return Ok(());
    }
    run_ai_tool(&root, tool, &args)?;
    Ok(())
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::ai::AiWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        if let Some(fw) = config.frameworks.first() {
            // Registry-first: fetch layer ai/<fw> nếu chưa có; fetch fail → fallback procedural.
            crate::commands::template::ensure_layer(&format!("ai/{fw}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success(
            "AI project created. Pull a model with `mg model pull hf://...` or run `mg dev`.",
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dev_routes_to_entry_script() {
        assert_eq!(
            mg_ai_adapter::AiFramework::PythonAgent.entry_script(),
            "src/agent.py"
        );
        assert_eq!(
            mg_ai_adapter::AiFramework::McpServer.entry_script(),
            "server.py"
        );
    }
}

#[cfg(test)]
mod add_tests {
    use super::*;

    #[test]
    fn lock_file_detects_uv_sync() {
        let dir = std::env::temp_dir().join(format!("mg-ai-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("uv.lock"), "ok").unwrap();
        let root = dir.clone();
        let (tool, args): (&str, Vec<String>) = if root.join("uv.lock").exists() {
            ("uv", vec!["sync".to_string()])
        } else {
            ("pip", Vec::new())
        };
        assert_eq!(tool, "uv");
        assert_eq!(args, vec!["sync".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
