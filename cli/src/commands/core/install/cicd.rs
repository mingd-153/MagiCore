//! `mgc install cicd` — bail rõ ràng (cicd không có install — dùng `mgc ci generate`/`verify`/`deploy`).

use anyhow::Result;

pub async fn install(_packages: Vec<String>, _dry_run: bool) -> Result<()> {
    Err(crate::error::cicd_verb_not_applicable("install"))
}
