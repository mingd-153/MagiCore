use anyhow::Result;
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
            "No MegaGate library project found (missing mg.toml with ecosystem = \"lib\" or Cargo.toml/package.json/pyproject.toml in the current project)"
        )
    })?;
    Ok(root)
}

fn lib_adapter() -> Arc<dyn PackageAdapter> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let (registry_url, token) = crate::context::ProjectContext::load_at(
        &cwd,
        mg_config::project::ProjectConfig::find_project_root(&cwd).as_ref(),
        None,
    )
    .ok()
    .map(|ctx| (ctx.config.registries.first().map(|r| r.url.clone()), ctx.config.registries.first().and_then(|r| r.token.clone())))
    .map(|(u, t)| {
        (
            u.or_else(|| std::env::var("MEGAGATE_LIB_REGISTRY_URL").ok()),
            t.or_else(|| std::env::var("MEGAGATE_LIB_REGISTRY_TOKEN").ok()),
        )
    })
    .unwrap_or_else(|| {
        (
            std::env::var("MEGAGATE_LIB_REGISTRY_URL").ok(),
            std::env::var("MEGAGATE_LIB_REGISTRY_TOKEN").ok(),
        )
    });
    crate::factory::create_adapter(&Ecosystem::Lib, registry_url.as_deref(), token.as_deref())
        .expect("lib adapter always available in lib core build")
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
    let adapter = lib_adapter();
    super::shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = lib_adapter();
    super::shared::remove(&*adapter, &root, packages, true).await
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let adapter = lib_adapter();
    super::shared::list(&*adapter, &root).await
}

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = lib_adapter();
    super::shared::update(&*adapter, &root, packages, install).await
}

pub async fn install(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = lib_adapter();
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

pub mod create {
    use anyhow::Result;

    pub async fn run(project_name: &str) -> Result<()> {
        let mut config = crate::wizard::lib::LibWizard::run();
        config.project_name = project_name.to_string();
        if let Some(lang) = config.frameworks.first() {
            // Registry-first: fetch layer lib/<lang> nếu chưa có; fetch fail → fallback procedural.
            crate::commands::template::ensure_layer(&format!("lib/{lang}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success("Library project created. Run `mg add-lib <pkg>` or `mg install-lib` next.");
        Ok(())
    }
}
