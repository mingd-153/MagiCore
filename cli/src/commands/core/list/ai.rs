//! `mgc list` ai — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = shared::ai_project_root()?;
    shared::require_native_ai_python(&root, "list")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    let adapter =
        crate::factory::create_adapter_for(&root, &mgc_types::Ecosystem::Ai, None, None, &[])
            .map_err(|e| anyhow::anyhow!("ai native list needs the PyPI engine: {e}"))?;
    crate::commands::dep_gate::gate_native_adapter(
        &crate::commands::dep_gate::DepContext::new(
            "ai",
            Some(crate::commands::dep_gate::eco::PYTHON),
            None,
            None,
            crate::commands::dep_gate::DepOp::List,
        ),
        &*adapter,
        &compat,
    )?;
    shared::list(&*adapter, &root).await
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
