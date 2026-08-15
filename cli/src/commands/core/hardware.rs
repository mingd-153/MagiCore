//! Hardware core — optimizer/bench packages (shared cho game/ai/cloud).
//! Không có native package manager: packages được materialize từ templates/hardware/.

use std::sync::Arc;
use anyhow::Result;
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;
use std::path::PathBuf;

pub const OPTIMIZER_PKG: &str = "optimizer";
pub const BENCH_PKG: &str = "bench";

pub async fn materialize_template(root: &PathBuf, framework: &str) -> Result<()> {
    let target_dir = root.join(framework);
    if target_dir.exists() {
        return Ok(()); // đã có — không ghi đè
    }
    // Registry-first: fetch layer hardware/<framework> nếu chưa có; fetch fail →
    // scaffold bên dưới bail rõ ràng (hardware không có fallback procedural).
    crate::commands::template::ensure_layer(&format!("hardware/{framework}")).await;
    let config = crate::wizard::engine::ScaffoldConfig {
        core: "hardware".to_string(),
        sub_type: String::new(),
        frameworks: vec![framework.to_string()],
        project_name: target_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    crate::scaffold::processor::Scaffolder::scaffold(&config)?;
    Ok(())
}

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| {
        anyhow::anyhow!("failed to resolve current working directory — has it been deleted?: {e}")
    })?;
    let root = super::shared::find_project_root(&cwd)?.ok_or_else(|| {
        anyhow::anyhow!(
            "No MegaGate project found (missing mg.toml or known project manifest in the current directory tree)"
        )
    })?;
    Ok(root)
}

fn hardware_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Hardware, None, None)
        .expect("hardware adapter always available in hardware core build")
}

/// `mg add-hardware <pkg>` — materialize optimizer/bench vào project.
pub async fn add(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = hardware_adapter();
    for pkg in &packages {
        match pkg.as_str() {
            OPTIMIZER_PKG | BENCH_PKG => {
                let spinner = mg_ui::create_spinner(&format!("  Materializing {pkg}..."));
                materialize_template(&root, pkg).await?;
                spinner.finish_and_clear();
                mg_ui::success(&format!("{pkg} scaffolded at ./{pkg}"));
            }
            other => anyhow::bail!("unknown hardware package '{other}' (optimizer | bench)"),
        }
    }
    super::shared::install_with_adapter(
        &*adapter,
        &root,
        "mg add-hardware",
        false,
        mg_types::adapter::InstallOptions::default(),
    )
    .await
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let adapter = hardware_adapter();
    super::shared::list(&*adapter, &root).await
}

pub async fn install(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    for pkg in &packages {
        match pkg.as_str() {
            OPTIMIZER_PKG | BENCH_PKG => materialize_template(&root, pkg).await?,
            other => anyhow::bail!("unknown hardware package '{other}' (optimizer | bench)"),
        }
    }
    let adapter = hardware_adapter();
    super::shared::install_with_adapter(
        &*adapter,
        &root,
        "mg install-hardware",
        false,
        mg_types::adapter::InstallOptions::default(),
    )
    .await
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::hardware::HardwareWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        match config.frameworks.first().map(|s| s.as_str()) {
            Some("optimizer") | Some("bench") => {}
            Some(other) => anyhow::bail!(
                "unknown hardware framework '{other}' (optimizer | bench) — template materialize thẳng vào project, không có project scaffold riêng"
            ),
            None => anyhow::bail!("no hardware framework selected"),
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success(&format!(
            "Hardware '{}' scaffolded at '{project_name}'. Run `mg add-hardware bench` hoặc `mg bench` để chạy benchmark.",
            config.frameworks.first().map(|s| s.as_str()).unwrap_or("")
        ));
        Ok(())
    }
}
