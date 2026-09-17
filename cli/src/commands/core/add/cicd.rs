//! `mgc add cicd` — bail rõ ràng (cicd không có add — dùng `mgc ci generate`).
//!
//! GATE-EXEMPT: no package lifecycle exists for this lane by design
//! (scaffold/pipeline only) — the bail below IS the fail-closed gate.
//! (GATE-EXEMPT: lane này không có package lifecycle theo thiết kế —
//! lệnh bail đã là cổng fail-closed.)

use anyhow::Result;

#[allow(clippy::too_many_arguments)]
pub async fn add(
    _packages: Vec<String>,
    _version: Option<String>,
    _dev: bool,
    _exact: bool,
    _optional: bool,
    _peer: bool,
    _no_save: bool,
    _global: bool,
    _compat_runtime: Option<String>,
) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("add"))
}
