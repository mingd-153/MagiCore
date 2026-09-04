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
            let strict_mode = std::env::var("MGC_AUDIT_STRICT")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false);

            eprintln!("⚠ Audit scanner unavailable: {}", reason);
            eprintln!("  Audit NOT performed - cannot verify security");

            if strict_mode {
                anyhow::bail!(
                    "Audit failed: scanner unavailable in strict mode\n\
                     Set MGC_AUDIT_STRICT=0 to allow unverified state (not recommended in CI)"
                );
            } else {
                eprintln!("  Status: UNVERIFIED (pass with warning)");
                eprintln!(
                    "  Set MGC_AUDIT_STRICT=1 to fail on unavailable scanner (recommended for CI)"
                );
                return Ok(());
            }
        }
    }

    if report.is_clean() {
        println!("  ✔   audit clean (template packages — no registry dependencies to check)");
    }
    Ok(())
}
