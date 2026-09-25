//! `mgc install cicd` — bail rõ ràng (cicd không có install — dùng `mgc ci generate`/`verify`/`deploy`).
//!
//! GATE-EXEMPT: no package lifecycle exists for this lane by design
//! (scaffold/pipeline only) — the bail below IS the fail-closed gate.
//! (GATE-EXEMPT: lane này không có package lifecycle theo thiết kế —
//! lệnh bail đã là cổng fail-closed.)

use anyhow::Result;

pub async fn install(
    _packages: Vec<String>,
    _dry_run: bool,
    _compat_runtime: Option<String>,
) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("install"))
}
