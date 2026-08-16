use anyhow::Result;
use std::path::PathBuf;

fn not_available(reason: &str) -> anyhow::Error {
    anyhow::anyhow!("'ai' {reason}")
}

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
    let _ = packages;
    Err(not_available(
        "has no package manager — install deps with pip (allowlist) inside your virtualenv: `pip install -r requirements.txt`.",
    ))
}
pub async fn remove(_packages: Vec<String>) -> Result<()> {
    Err(not_available(
        "has no package manager — remove deps with pip yourself.",
    ))
}
pub async fn list() -> Result<()> {
    Err(not_available(
        "has no package registry — inspect deps with `pip list` / `pip freeze`.",
    ))
}
pub async fn update(_packages: Vec<String>, _install: bool) -> Result<()> {
    Err(not_available(
        "dep updates flow through pip — run `pip install --upgrade` yourself.",
    ))
}
pub async fn install(_packages: Vec<String>) -> Result<()> {
    Err(not_available(
        "has no managed install — run `pip install -r requirements.txt` in your virtualenv.",
    ))
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
