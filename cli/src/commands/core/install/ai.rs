//! `mgc install` ai — tách từ core/ai.rs (Phase 7 v5). Lock ghép: uv.lock → pip requirements.lock.

use anyhow::Result;

use super::super::shared;

pub async fn install(
    packages: Vec<String>,
    dry_run: bool,
    compat_runtime: Option<String>,
    frozen: bool,
) -> Result<()> {
    let root = shared::ai_project_root()?;
    shared::require_native_ai_python(&root, "install")?;
    if !packages.is_empty() {
        return Err(crate::error::native_dependency_engine_unavailable(
            "ai",
            "python package arguments",
            "install; use `mgc add ai` to declare packages",
        ));
    }
    if dry_run {
        mgc_ui::info("[dry-run] would run native AI/Python dependency install");
        return Ok(());
    }
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    let adapter =
        crate::factory::create_adapter_for(&root, &mgc_types::Ecosystem::Ai, None, None, &[])
            .map_err(|e| anyhow::anyhow!("ai native install needs the PyPI engine: {e}"))?;
    crate::commands::dep_gate::gate_native_adapter(
        &crate::commands::dep_gate::DepContext::new(
            "ai",
            Some(crate::commands::dep_gate::eco::PYTHON),
            None,
            None,
            crate::commands::dep_gate::DepOp::Install,
        ),
        &*adapter,
        &compat,
    )?;
    crate::commands::core::shared::install_with_adapter(
        &*adapter,
        &root,
        "mgc add",
        frozen,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
