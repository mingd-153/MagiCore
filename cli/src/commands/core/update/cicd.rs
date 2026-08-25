//! `mgc update cicd` — bail rõ ràng (cicd không có update).

use anyhow::Result;

pub async fn update(_packages: Vec<String>, _install: bool) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("update"))
}
