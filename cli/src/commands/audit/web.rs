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
        let fixed = adapter.audit_fix(project_root, &ids).await?;
        mgc_ui::success(&format!(
            "audit --fix bumped {} package(s); lockfile rewritten",
            fixed
        ));
    }
    Ok(())
}
