//! `mgc list app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{language, project_root};

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    if !matches!(
        lang,
        mgc_app_adapter::AppLanguage::Flutter
            | mgc_app_adapter::AppLanguage::Swift
            | mgc_app_adapter::AppLanguage::ReactNative
    ) {
        return Err(crate::error::native_dependency_engine_unavailable(
            "app",
            lang.ecosystem(),
            "list",
        ));
    }
    let adapter =
        mgc_app_adapter::adapter_for(&root).ok_or_else(crate::error::app_project_not_detected)?;
    crate::commands::dep_gate::gate_native_adapter(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(lang.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::List,
        ),
        &adapter,
        &compat,
    )?;
    crate::commands::core::shared::list(&adapter, &root).await
}
