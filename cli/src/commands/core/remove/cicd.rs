//! `mgc remove cicd` — bail rõ ràng (cicd không có remove).
//!
//! GATE-EXEMPT: no package lifecycle exists for this lane by design —
//! the bail below IS the fail-closed gate.
//! (GATE-EXEMPT: lane này không có package lifecycle theo thiết kế.)

use anyhow::Result;

pub async fn remove(_packages: Vec<String>, _compat_runtime: Option<String>) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("remove"))
}
