//! `mgc remove app` — native Flutter dependency mutation; unsupported lanes fail closed.
//! `mgc remove app` — Flutter sửa dependency native; lane chưa hỗ trợ bị chặn.

use anyhow::Result;

use crate::commands::core::install::app::{
    gate_react_native, language, manifest_hint, project_root,
};

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    if packages.is_empty() {
        return Err(crate::error::remove_app_usage());
    }
    // Flutter remove uses the shared journaled manifest writer. Swift and
    // Kotlin mutations stay blocked in the ownership gate until their
    // source/catalog edits use the same crash-recovery transaction.
    // (Flutter qua journal chung; Swift/Kotlin mutation đang fail-closed.)
    if lang == mgc_app_adapter::AppLanguage::Flutter {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "app",
                Some(lang.ecosystem()),
                None,
                None,
                crate::commands::dep_gate::DepOp::Remove,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let adapter = mgc_app_adapter::adapter_for(&root)
            .ok_or_else(crate::error::app_project_not_detected)?;
        return crate::commands::core::shared::remove(&adapter, &root, packages, true).await;
    }
    // Reject unsupported languages before any process spawn.
    // Từ chối ngôn ngữ chưa hỗ trợ trước mọi lần spawn.
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    gate_react_native(
        &root,
        lang,
        crate::commands::dep_gate::DepOp::Remove,
        &compat,
    )?;
    // C0 ownership firewall (T0.3): single control path.
    // (Tường lửa C0: đường điều khiển duy nhất.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(lang.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::Remove,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    Err(manifest_hint(lang, "remove"))
}
