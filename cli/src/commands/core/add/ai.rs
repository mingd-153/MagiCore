//! `mgc add` ai — native PyPI dependency lane.

use super::super::shared;
use anyhow::Result;

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
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = shared::ai_project_root()?;
    shared::require_native_ai_python(&root, "add")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    let adapter =
        crate::factory::create_adapter_for(&root, &mgc_types::Ecosystem::Ai, None, None, &[])
            .map_err(|e| anyhow::anyhow!("ai native add needs the PyPI engine: {e}"))?;
    crate::commands::dep_gate::gate_native_adapter(
        &crate::commands::dep_gate::DepContext::new(
            "ai",
            Some(crate::commands::dep_gate::eco::PYTHON),
            None,
            None,
            crate::commands::dep_gate::DepOp::Add,
        ),
        &*adapter,
        &compat,
    )?;
    crate::commands::core::shared::add(
        &*adapter, &root, packages, _version, _dev, _exact, _optional, _peer, _no_save, true,
        _global,
    )
    .await
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
