//! `mgc list cicd` — bail rõ ràng (cicd không có list).
//!
//! GATE-EXEMPT: no package lifecycle exists for this lane by design —
//! the bail below IS the fail-closed gate. The --compat-runtime flag is
//! accepted (CLI uniformity, P0#3) and ignored: no flag can open a lane
//! that does not exist.
//! (GATE-EXEMPT: lane này không có package lifecycle theo thiết kế.)

use anyhow::Result;

pub async fn list(_compat_runtime: Option<String>) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("list"))
}
