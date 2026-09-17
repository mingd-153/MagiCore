//! `mgc remove iot` — tách từ core/iot.rs (Phase 7 v5).

use anyhow::Result;
use mgc_types::Ecosystem;
use mgc_types::adapter::PackageAdapter;
use std::path::PathBuf;
use std::sync::Arc;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("iot"))?;
    Ok(root)
}

fn iot_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Iot, None, None)
        .expect("iot adapter always available in iot core build")
}

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): the IoT remove lane routes to the
    // adapter, whose frameworks delegate (cargo/pio/west).
    // (Tường lửa C0: lane remove IoT gọi adapter, framework trong đó
    // delegate.)
    crate::commands::dep_gate::gate(
        "iot",
        crate::commands::dep_gate::DepOp::Remove,
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = iot_adapter();
    shared::remove(&*adapter, &root, packages, true).await
}
