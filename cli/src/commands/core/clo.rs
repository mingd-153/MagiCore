use anyhow::{bail, Result};
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;
use std::path::PathBuf;
use std::sync::Arc;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| {
        anyhow::anyhow!("failed to resolve current working directory — has it been deleted?: {e}")
    })?;
    let root = super::shared::find_project_root(&cwd)?.ok_or_else(|| {
        anyhow::anyhow!(
            "No MegaGate cloud project found (missing mg.toml with ecosystem = \"cloud\" or Pulumi.yaml/*.tf/cdk package.json in the current project)"
        )
    })?;
    Ok(root)
}

fn cloud_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Cloud, None, None)
        .expect("cloud adapter always available in clo core build")
}

/// Cloud type từ mg.toml `[cloud] type` hoặc manifest probe — dùng cho dev/deploy.
pub fn cloud_type(root: &PathBuf) -> anyhow::Result<String> {
    let adapter = mg_cloud_adapter::adapter_for(root)
        .ok_or_else(|| anyhow::anyhow!("No cloud framework detected in {}", root.display()))?;
    Ok(adapter.cloud_type().to_string())
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
) -> Result<()> {
    let root = project_root()?;
    let adapter = cloud_adapter();
    super::shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = cloud_adapter();
    super::shared::remove(&*adapter, &root, packages, true).await
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let adapter = cloud_adapter();
    super::shared::list(&*adapter, &root).await
}

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = cloud_adapter();
    super::shared::update(&*adapter, &root, packages, install).await
}

pub async fn install(packages: Vec<String>, dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = cloud_adapter();
    if dry_run {
        // terraform passthrough — in lệnh init/get KHÔNG chạy (spec §5 criterion)
        if !packages.is_empty() {
            mg_ui::info(&format!(
                "[dry-run] ignoring package args {:?} — terraform install = init/get",
                packages
            ));
        }
        let kind = cloud_type(&root)?;
        if kind != "terraform" {
            mg_ui::info(&format!(
                "[dry-run] would run npm-registry install via mg-resolver for {kind}"
            ));
            return Ok(());
        }
        mg_ui::info("[dry-run] would run: terraform init");
        mg_ui::info("[dry-run] would run: terraform get");
        return Ok(());
    }
    for pkg in &packages {
        let spinner = mg_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mg_types::PackageName::new(pkg)?;
        let opts = mg_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    super::shared::install_with_adapter(
        &*adapter,
        &root,
        "mg add",
        false,
        mg_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}

pub async fn dev(dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let kind = cloud_type(&root)?;
    let (cmd, args) = dev_command(&kind)?;
    if dry_run {
        mg_ui::info(&format!("[dry-run] would run: {} {}", cmd, args.join(" ")));
        return Ok(());
    }
    run_tool(&root, &cmd, &args)?;
    Ok(())
}

/// `mg deploy` — mặc định dry-run (in lệnh deploy theo type, KHÔNG chạy);
/// chạy thật chỉ với `--run` (spec §4: deploy = hành động ghi cloud).
pub async fn deploy(run: bool) -> Result<()> {
    let root = project_root()?;
    let kind = cloud_type(&root)?;
    let (cmd, args) = deploy_command(&kind)?;
    if !run {
        mg_ui::info(&format!(
            "[dry-run] would run: {} {} (deploy chạy thật cần `mg deploy --run`)",
            cmd,
            args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!("Deploying: {} {}", cmd, args.join(" ")));
    run_tool(&root, &cmd, &args)?;
    Ok(())
}

fn dev_command(kind: &str) -> Result<(String, Vec<String>)> {
    match kind {
        "terraform" => Ok(("terraform".to_string(), vec!["plan".to_string()])),
        other => bail!(
            "'mg dev' for '{other}' cloud type is not implemented yet — terraform (plan) is P1; cdk (synth) / pulumi (preview) cần chạy trong node_modules, P2"
        ),
    }
}

pub fn deploy_command(kind: &str) -> Result<(String, Vec<String>)> {
    match kind {
        "terraform" => Ok(("terraform".to_string(), vec!["apply".to_string()])),
        "cdk" => Ok(("cdk".to_string(), vec!["deploy".to_string()])),
        "pulumi" => Ok(("pulumi".to_string(), vec!["up".to_string()])),
        other => anyhow::bail!("'mg deploy' for '{other}' cloud type is not implemented yet"),
    }
}

fn run_tool(root: &PathBuf, cmd: &str, args: &[String]) -> Result<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.clone()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run_inherited(cmd, args, &opts)?;
    Ok(())
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::cloud::CloudWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        if let Some(fw) = config.frameworks.first() {
            // Registry-first: fetch layer clo/<fw> nếu chưa có; fetch fail → fallback procedural.
            crate::commands::template::ensure_layer(&format!("clo/{fw}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success("Cloud project created. Run `mg add-clo <pkg>` or `mg install-clo` next.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_command_maps_types() {
        let (cmd, args) = dev_command("terraform").unwrap();
        assert_eq!(cmd, "terraform");
        assert_eq!(args, vec!["plan"]);
        assert!(dev_command("cdk").is_err());
        assert!(dev_command("pulumi").is_err());
        assert!(dev_command("unknown").is_err());
    }

    #[test]
    fn deploy_command_maps_types() {
        let (cmd, args) = deploy_command("terraform").unwrap();
        assert_eq!(cmd, "terraform");
        assert_eq!(args, vec!["apply"]);
        let (cmd, args) = deploy_command("cdk").unwrap();
        assert_eq!(cmd, "cdk");
        assert_eq!(args, vec!["deploy"]);
        let (cmd, args) = deploy_command("pulumi").unwrap();
        assert_eq!(cmd, "pulumi");
        assert_eq!(args, vec!["up"]);
    }
}
