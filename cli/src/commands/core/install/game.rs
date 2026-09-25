//! `mgc install` game — dependency work runs only through the native Lib/Rust lane.

use anyhow::Result;
use mgc_types::Ecosystem;

pub async fn install(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let engine = mgc_game_adapter::adapter_for(&root).map(|adapter| adapter.engine());

    if engine == Some("bevy") && root.join("Cargo.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "game",
                Some("bevy"),
                Some("bevy"),
                None,
                crate::commands::dep_gate::DepOp::Install,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let lib_adapter =
            crate::factory::create_adapter(&Ecosystem::Lib, None, None).map_err(|error| {
                anyhow::anyhow!("game native install needs the Lib/Rust engine: {error}")
            })?;
        if !packages.is_empty() {
            crate::commands::core::shared::add(
                &*lib_adapter,
                &root,
                packages,
                None,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
            )
            .await?;
        }
        return crate::commands::core::shared::install_with_adapter(
            &*lib_adapter,
            &root,
            "mgc add",
            false,
            mgc_types::adapter::InstallOptions {
                legacy_flat: false,
                ..Default::default()
            },
        )
        .await;
    }

    if engine == Some("bevy") {
        anyhow::bail!(
            "native Bevy dependency installation requires Cargo.toml at the project root"
        );
    }

    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "game",
            engine,
            engine,
            None,
            crate::commands::dep_gate::DepOp::Install,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    anyhow::bail!("MagiCore does not yet own dependency installation for this game engine")
}
