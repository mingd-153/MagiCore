//! `mg remove cicd` — bail rõ ràng (cicd không có remove).

use anyhow::Result;

pub async fn remove(_packages: Vec<String>) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("remove"))
}
