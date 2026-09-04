use anyhow::Result;
use mgc_types::Ecosystem;

pub async fn audit() -> Result<()> {
    let ctx = crate::context::ProjectContext::load_with_core(Some("hardware"))?;
    let adapter = crate::factory::create_adapter(&Ecosystem::Hardware, None, None)?;
    let report = adapter.audit(ctx.root()).await?;
    let pkgs = adapter.list(ctx.root()).await?;

    println!("  ℹ   hardware packages: {}", pkgs.len());
    for p in &pkgs {
        println!("      {}@{}", p.id.name().as_str(), p.id.version());
    }

    // P0.6 FIX: Check scanner availability before reporting clean
    if !report.scanner_available() {
        if let mgc_types::adapter::ScannerStatus::Unavailable(reason) = &report.scanner_status {
            println!("  ⚠   audit scanner unavailable: {}", reason);
            println!("      Audit NOT performed - scanner status returned instead of fake clean");
            return Ok(());
        }
    }

    if report.is_clean() {
        println!("  ✔   audit clean (template packages — no registry dependencies to check)");
    }
    Ok(())
}
