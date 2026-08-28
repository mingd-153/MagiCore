//! `mgc add cicd` — bail rõ ràng (cicd không có add — dùng `mgc ci generate`).

use anyhow::Result;

pub async fn add(
    _packages: Vec<String>,
    _version: Option<String>,
    _dev: bool,
    _exact: bool,
    _optional: bool,
    _peer: bool,
    _no_save: bool,
    _global: bool,
) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("add"))
}
