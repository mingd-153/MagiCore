//! `mgc remove` ai — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    if packages.is_empty() {
        return Err(crate::error::remove_ai_usage());
    }
    let root = shared::ai_project_root()?;
    shared::require_native_ai_python(&root, "remove")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    let adapter =
        crate::factory::create_adapter_for(&root, &mgc_types::Ecosystem::Ai, None, None, &[])
            .map_err(|e| anyhow::anyhow!("ai native remove needs the PyPI engine: {e}"))?;
    crate::commands::dep_gate::gate_native_adapter(
        &crate::commands::dep_gate::DepContext::new(
            "ai",
            Some(crate::commands::dep_gate::eco::PYTHON),
            None,
            None,
            crate::commands::dep_gate::DepOp::Remove,
        ),
        &*adapter,
        &compat,
    )?;
    shared::remove(&*adapter, &root, packages, true).await
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
