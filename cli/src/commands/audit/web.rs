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
    use super::{enforce_audit_exit, print_audit_report};
    let strict = StrictMode::from_env();
    let report = adapter.audit(project_root).await?;

    // Defer only when a fix will actually run; every no-op/error branch
    // must still emit exactly one machine document. (Chỉ hoãn khi chắc
    // chắn có mutate; nhánh không sửa vẫn phải phát đúng một document.)
    if should_print_initial_report(
        fix,
        fmt,
        report.scanner_available(),
        report.vulnerability_count,
    ) {
        print_audit_report("web", &report, fmt)?;
    }
    if !fix {
        return enforce_audit_exit("web", &report, strict);
    }
    // --fix flow (P0-1): render first, mutate, re-audit, THEN exit.
    // Only Available reports auto-fix — Partial/Failed/ToolMissing never
    // do (fail-closed: an unverified scan must not rewrite dependencies).
    // (Chỉ Available mới tự fix — Partial/Failed không bao giờ.)
    if !report.scanner_available() {
        return enforce_audit_exit("web", &report, strict);
    }
    if report.vulnerability_count == 0 {
        return enforce_audit_exit("web", &report, strict);
    }
    let ids: Vec<_> = report
        .vulnerabilities
        .iter()
        .map(|v| v.package.clone())
        .collect();
    // Transaction gateway: audit --fix rewrites manifest + lock — a real
    // mutation, journaled like any other (snapshot, stage, post-image,
    // finish/rollback). Never a bare adapter call.
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
    // Post-stage boundary: every fallible step routes through rollback
    // (same contract as add/remove/update/install).
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
    if !fmt.is_machine() {
        mgc_ui::success(&format!(
            "audit --fix bumped {} package(s); lockfile rewritten",
            fixed
        ));
    }
    // Re-audit AFTER the fix and exit on the POST report (the pre-fix
    // findings no longer describe the project).
    // (Audit lại sau fix — exit theo report mới.)
    let post = adapter.audit(project_root).await?;
    print_audit_report("web", &post, fmt)?;
    enforce_audit_exit("web", &post, strict)
}

pub(super) fn should_print_initial_report(
    fix: bool,
    fmt: OutputFormat,
    scanner_available: bool,
    findings: usize,
) -> bool {
    !(fix && fmt.is_machine() && scanner_available && findings > 0)
}
