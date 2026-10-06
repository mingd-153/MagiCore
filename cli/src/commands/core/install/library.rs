//! `mgc install library` — tách từ core/library.rs (Phase 7 v5).

use anyhow::Result;
fn project_root() -> Result<PathBuf> {
    shared::core_project_root("lib")
}

use std::path::PathBuf;

use crate::commands::core::shared;

pub async fn install(
    packages: Vec<String>,
    compat_runtime: Option<String>,
    frozen: bool,
    offline: bool,
) -> Result<()> {
    let root = project_root()?;
    // P0 install/add split: `install-lib` NEVER adds packages — it replays
    // the existing graph/lock through the native pipeline. Package args
    // previously flowed into `adapter.add()` under the Install gate, so the
    // command could mutate the manifest instead of replaying the lock. Fail
    // closed with the exact command instead.
    // (P0: install-lib KHÔNG BAO GIỜ add package — chỉ cài graph/lock
    // hiện có. Package args phải đi `add-lib`.)
    if !packages.is_empty() {
        return Err(crate::error::install_lib_packages_use_add(&packages));
    }
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): TypeScript rides the native web engine;
    // protocol languages run the native resolve/fetch/CAS pipeline
    // (adapters/lib/src/install spawns NO toolchain — verified by scan).
    // (Tường lửa C0: pipeline native, không spawn toolchain.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::lib_project_context(
            &root,
            if offline {
                crate::commands::dep_gate::DepOp::OfflineReinstall
            } else {
                crate::commands::dep_gate::DepOp::Install
            },
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = shared::lib_adapter(&root)?;
    shared::install_with_adapter(
        &*adapter,
        &root,
        "mgc add",
        frozen || offline,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            offline,
            ..Default::default()
        },
    )
    .await
}
