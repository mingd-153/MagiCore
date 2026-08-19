//! `mg dev` ai — chạy entry script qua python3 (Q20, allowlist §5.1) — tách từ core/ai.rs.

use anyhow::Result;

use super::super::shared;

pub async fn dev(dry_run: bool) -> Result<()> {
    shared::ai_dev(dry_run).await
}
