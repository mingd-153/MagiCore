//! `mgc add` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = shared::core_project_root("clo")?;
    // C0 ownership firewall (T0.3): terraform delegates; the CDK/Pulumi
    // branch rides the native web engine and skips the gate.
    // (Tường lửa C0: terraform delegate; nhánh CDK/Pulumi đi engine web
    // native nên không qua gate.)
    let cloud_kind = super::super::dev::clo::cloud_type(&root)?;
    if cloud_kind == "terraform" {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            "clo",
            crate::commands::dep_gate::DepOp::Add,
            Some("terraform"),
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
    }
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}
