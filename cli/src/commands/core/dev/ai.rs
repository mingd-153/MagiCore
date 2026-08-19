//! `mg dev` ai — chạy entry script qua python3 (Q20, allowlist §5.1) — tách từ core/ai.rs.

use anyhow::Result;

use super::super::shared;

pub async fn dev(dry_run: bool) -> Result<()> {
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(Some(root)) = crate::commands::core::shared::find_project_root(&cwd) {
            // Tự động sinh Dockerfile và docker-compose.yml nếu chưa có
            let _ = crate::commands::core::dev::ai_docker::generate_ai_docker_files(&root);
        }
    }
    shared::ai_dev(dry_run).await
}
