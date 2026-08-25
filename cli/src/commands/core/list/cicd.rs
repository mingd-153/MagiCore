//! `mgc list cicd` — bail rõ ràng (cicd không có list).

use anyhow::Result;

pub async fn list() -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("list"))
}
