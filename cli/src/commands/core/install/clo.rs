//! `mgc install` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::dev::clo as clo_tools;
use super::super::shared;
use mgc_types::Ecosystem;

// DELEGATED: terraform init/get (or the CDK/Pulumi web-engine lane) runs
// for real — mgc does not own this dependency lifecycle.
// (DELEGATED: terraform init/get (hoặc lane web-engine CDK/Pulumi) chạy
// thật — mgc không sở hữu lifecycle dependency này.)
pub async fn install(
    packages: Vec<String>,
    dry_run: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    if dry_run {
        // terraform passthrough — in lệnh init/get KHÔNG chạy (spec §5 criterion)
        if !packages.is_empty() {
            mgc_ui::info(&format!(
                "[dry-run] ignoring package args {:?} — terraform install = init/get",
                packages
            ));
        }
        let cloud_kind = clo_tools::cloud_type(&root)?;
        if cloud_kind != "terraform" {
            mgc_ui::info(&format!(
                "[dry-run] would run npm-registry install via mgc-resolver for {cloud_kind}"
            ));
            return Ok(());
        }
        mgc_ui::info("[dry-run] would run: terraform init");
        mgc_ui::info("[dry-run] would run: terraform get");
        return Ok(());
    }
    // C0 ownership firewall (T0.3): terraform init/get delegates; the
    // CDK/Pulumi branch rides the native web engine and skips the gate.
    // (Tường lửa C0: terraform init/get delegate; nhánh CDK/Pulumi đi
    // engine web native nên không qua gate.)
    let cloud_kind = clo_tools::cloud_type(&root)?;
    if cloud_kind == "terraform" {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            "clo",
            crate::commands::dep_gate::DepOp::Install,
            Some("terraform"),
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
    }
    for pkg in &packages {
        let spinner = mgc_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mgc_types::PackageName::new(pkg)?;
        let opts = mgc_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    shared::install_with_adapter(
        &*adapter,
        &root,
        "mgc add",
        false,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}
