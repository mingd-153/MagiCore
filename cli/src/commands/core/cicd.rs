use anyhow::Result;

fn not_available(reason: &str) -> anyhow::Error {
    anyhow::anyhow!("'cicd' {reason}")
}

fn provider_config() -> Result<mg_cicd_adapter::CicdProvider> {
    let cwd = std::env::current_dir()?;
    mg_cicd_adapter::adapter_for(&cwd)
        .map(|a| a.provider)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot detect a cicd project here (missing mg.toml [cicd] provider / wrangler.toml / argocd / .github/workflows)."
            )
        })
}

/// Deploy command theo provider — dry-run mặc định, --run để chạy thật (§5.4/S2).
struct DeployCommand {
    tool: &'static str,
    args: Vec<String>,
}

fn deploy_command(provider: mg_cicd_adapter::CicdProvider) -> Result<DeployCommand> {
    match provider {
        mg_cicd_adapter::CicdProvider::Cloudflare => Ok(DeployCommand {
            tool: "wrangler",
            args: vec!["deploy".to_string()],
        }),
        mg_cicd_adapter::CicdProvider::Gcp => Ok(DeployCommand {
            tool: "gcloud",
            args: vec!["app".to_string(), "deploy".to_string()],
        }),
        mg_cicd_adapter::CicdProvider::GithubActions => Err(not_available(
            "github-actions is CI-only — push to trigger; no local deploy command.",
        )),
        mg_cicd_adapter::CicdProvider::Aws => Err(not_available(
            "aws deploy needs a target (s3 bucket/pipeline) — configure target then use `mg exec aws ...`.",
        )),
        mg_cicd_adapter::CicdProvider::Argocd => Err(not_available(
            "argocd runs server-side (GitOps) — commit + push to trigger sync; no local deploy command.",
        )),
    }
}

pub async fn deploy(run: bool) -> Result<()> {
    let root = std::env::current_dir()?;
    let provider = provider_config()?;
    let cmd = deploy_command(provider)?;

    if !run {
        mg_ui::info(&format!(
            "[dry-run] would run: {} {} (deploy chạy thật cần `mg deploy --run`)",
            cmd.tool,
            cmd.args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!("Deploying: {} {}", cmd.tool, cmd.args.join(" ")));
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.clone()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run_inherited(cmd.tool, &cmd.args, &opts)?;
    Ok(())
}

/// `mg dev` cho cicd — in lệnh deploy (dry-run), không chạy thật (§5.4/S2).
pub async fn dev(_dry_run: bool) -> Result<()> {
    let root = std::env::current_dir()?;
    let provider = provider_config()?;
    let cmd = deploy_command(provider)?;
    mg_ui::info(&format!(
        "[dry-run] preview: {} {} (run with `mg deploy --run`)",
        cmd.tool,
        cmd.args.join(" ")
    ));
    let _ = root;
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
        "has no package manager — deploy through `mg deploy` (dry-run default).",
    ))
}
pub async fn remove(_packages: Vec<String>) -> Result<()> {
    Err(not_available("has no package manager."))
}
pub async fn list() -> Result<()> {
    Err(not_available(
        "has no packages — preview deploy with `mg deploy`.",
    ))
}
pub async fn update(_packages: Vec<String>, _install: bool) -> Result<()> {
    Err(not_available(
        "has no package manager — update via provider CLI (wrangler/aws/gh/gcloud).",
    ))
}
pub async fn install(_packages: Vec<String>, _dry_run: bool) -> Result<()> {
    Err(not_available(
        "has no dependencies to install — run `mg deploy` instead.",
    ))
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::cicd::CicdWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        if let Some(fw) = config.frameworks.first() {
            // Registry-first: fetch layer cicd/<fw> nếu chưa có; fetch fail → fallback procedural.
            crate::commands::template::ensure_layer(&format!("cicd/{fw}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success("CICD project created. Run `mg deploy` (dry-run) to preview deployment.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_deploy_command() {
        let cmd = deploy_command(mg_cicd_adapter::CicdProvider::Cloudflare).expect("cloudflare ok");
        assert_eq!(cmd.tool, "wrangler");
        assert_eq!(cmd.args, vec!["deploy"]);
    }

    #[test]
    fn gcp_deploy_command() {
        let cmd = deploy_command(mg_cicd_adapter::CicdProvider::Gcp).expect("gcp ok");
        assert_eq!(cmd.tool, "gcloud");
        assert_eq!(cmd.args, vec!["app", "deploy"]);
    }

    #[test]
    fn ci_only_providers_bail() {
        assert!(deploy_command(mg_cicd_adapter::CicdProvider::GithubActions).is_err());
        assert!(deploy_command(mg_cicd_adapter::CicdProvider::Aws).is_err());
        assert!(deploy_command(mg_cicd_adapter::CicdProvider::Argocd).is_err());
    }
}
