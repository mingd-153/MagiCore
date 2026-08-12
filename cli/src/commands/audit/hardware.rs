use anyhow::Result;
use mg_types::Ecosystem;

pub async fn audit() -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(Some("hardware"))?;
    let adapter = crate::factory::create_adapter(&Ecosystem::Hardware, None, None)?;
    let report = adapter.audit(ctx.root()).await?;
    let pkgs = adapter.list(ctx.root()).await?;

    println!("  ℹ   hardware packages: {}", pkgs.len());
    for p in &pkgs {
        println!("      {}@{}", p.id.name().as_str(), p.id.version());
    }
    if report.is_clean() {
        println!("  ✔   audit clean (template packages — no registry dependencies to check)");
    }
    Ok(())
}
