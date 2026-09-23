//! Web core audit — adapter + lockfile audit with the shared finisher.
//! Audit core Web — adapter + lockfile audit, dùng finisher chung cho exit
//! contract (fix path vẫn riêng vì chỉ web có audit_fix).

use super::{OutputFormat, StrictMode};
use anyhow::Result;
use std::path::Path;

pub(crate) async fn audit(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    project_root: &Path,
    fix: bool,
    fmt: OutputFormat,
) -> Result<()> {
    let report = adapter.audit(project_root).await?;
    super::finish_and_print("web", &report, StrictMode::from_env(), fmt).await?;

    // The finisher enforces the exit contract; the fix path runs after a
    // finding report and rewrites the lockfile (web-only capability).
    // Fix path: chạy qua finisher giữ exit contract; path fix chạy sau
    // report có finding và viết lại lockfile (capability riêng của web).
    if report.vulnerability_count > 0 && fix {
        let ids: Vec<_> = report
            .vulnerabilities
            .iter()
            .map(|v| v.package.clone())
            .collect();
        // Transaction gateway (static-gate finding): audit --fix rewrites
        // manifest + lock — a real mutation, journaled like any other
        // (snapshot, stage, post-image, finish/rollback). Never a bare
        // adapter call.
        // (audit --fix qua gateway + journal như mọi mutation.)
        let write_lock = crate::commands::core::shared::begin_dependency_mutation(
            adapter,
            project_root,
            crate::commands::core::shared::MutationOperation::AuditFix,
        )
        .await?;
        let snapshot = crate::commands::core::shared::MutationSnapshot::capture(
            &adapter.parse_manifest(project_root).await?,
            project_root,
            &write_lock,
            crate::commands::core::shared::MutationOperation::AuditFix,
        )?;
        crate::commands::core::shared::stage_mutation_journal(
            project_root,
            adapter,
            &ids.iter()
                .map(|id| id.name_str().to_string())
                .collect::<Vec<_>>(),
            &snapshot,
            &write_lock,
        )?;
        // Post-stage boundary: every fallible step routes through
        // rollback (same contract as add/remove/update/install).
        // (Mọi bước post-stage qua rollback.)
        let fixed = match adapter.audit_fix(project_root, &ids).await {
            Ok(fixed) => fixed,
            Err(e) => {
                return crate::commands::core::shared::rollback_mutation(
                    adapter,
                    project_root,
                    &snapshot,
                    e,
                    &write_lock,
                )
                .await;
            }
        };
        if let Err(e) = crate::commands::core::shared::record_post_image(
            project_root,
            &adapter.parse_manifest(project_root).await?,
            &write_lock,
        ) {
            return crate::commands::core::shared::rollback_mutation(
                adapter,
                project_root,
                &snapshot,
                e,
                &write_lock,
            )
            .await;
        }
        if let Err(e) =
            crate::commands::core::shared::finish_mutation_journal(project_root, &write_lock)
        {
            return crate::commands::core::shared::rollback_mutation(
                adapter,
                project_root,
                &snapshot,
                e,
                &write_lock,
            )
            .await;
        }
        mgc_ui::success(&format!(
            "audit --fix bumped {} package(s); lockfile rewritten",
            fixed
        ));
    }
    Ok(())
}
