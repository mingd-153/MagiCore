//! `mgc migrate` — explicit lockfile migrations (V1.2 §16.4).
//! Migration lockfile tường minh.
//!
//! v4 is not currently writable from v1-v3: the install, mutation, and
//! audit paths still consume the legacy lock schema. The explicit migration
//! command therefore refuses without changing the existing lock until all
//! runtime consumers can preserve v4 package identity. Existing v4 files are
//! accepted as a no-op.

use anyhow::Result;
use clap::Subcommand;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Subcommand, Debug, Clone)]
pub enum MigrateCmd {
    /// Migrate mgc.lock (v4 writes stay disabled until runtime support is complete)
    Lock {
        /// Target schema version (only "4")
        #[arg(long)]
        to: String,
        /// Target project directory (default: cwd project)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

/// Default writer-lock acquire timeout (overridable by
/// `mgc.toml [lock].acquire_timeout_ms`).
const DEFAULT_ACQUIRE_TIMEOUT_MS: u64 = 30_000;

fn acquire_timeout_ms(root: &Path) -> u64 {
    mgc_config::project::ProjectConfig::load(root)
        .ok()
        .flatten()
        .and_then(|cfg| cfg.lock)
        .and_then(|lock| lock.acquire_timeout_ms)
        .unwrap_or(DEFAULT_ACQUIRE_TIMEOUT_MS)
}

pub async fn run(cmd: MigrateCmd) -> Result<()> {
    match cmd {
        MigrateCmd::Lock { to, dir } => run_lock(dir, &to).await,
    }
}

async fn run_lock(dir: Option<PathBuf>, to: &str) -> Result<()> {
    // Help text, error message, and module docs all say `--to v4` — accept
    // the documented spelling (strip one leading 'v'); bare "4" keeps
    // working. Rejecting the documented form was a self-contradiction.
    // (Chấp nhận cả "v4" và "4" — tài liệu ghi v4.)
    let normalized = to.strip_prefix('v').unwrap_or(to);
    if normalized != "4" {
        return Err(crate::error::migrate_unknown_target(to));
    }
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = dir
        .map(|d| mgc_config::project::ProjectConfig::find_project_root(&d).unwrap_or(d))
        .unwrap_or(cwd);
    let root = mgc_config::project::ProjectConfig::find_project_root(&root).unwrap_or(root);
    // Writer lock FIRST: even a read-only migration refusal must not race a
    // concurrent mutation while inspecting the lock's schema version.
    // (Acquire lock trước khi kiểm tra schema để tránh đọc giữa lúc mutation.)
    let guard = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        &root,
        Duration::from_millis(acquire_timeout_ms(&root)),
    )
    .map_err(|e| anyhow::anyhow!("migrate cannot acquire the project writer lock: {e}"))?;
    // Mutation gateway parity: never migrate over an unrestored mutation
    // journal (P0) — recover first via remove/install, then migrate.
    // (Không migrate đè lên journal chưa phục hồi.)
    crate::commands::core::shared::ensure_no_pending_remove_journal(&root, &guard)?;
    let lock_path = root.join("mgc.lock");
    let lock_bytes = match mgc_lockfile::read_lockfile_bytes(&lock_path) {
        Ok(bytes) => bytes,
        Err(mgc_lockfile::LockfileError::IoError(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Err(crate::error::migrate_no_lockfile(&root));
        }
        Err(error) => return Err(error.into()),
    };
    let text = String::from_utf8(lock_bytes)
        .map_err(|error| anyhow::anyhow!("mgc.lock is not valid UTF-8: {error}"))?;
    // Do not emit v4 until all runtime consumers can read it losslessly.
    // (Chưa ghi v4 khi runtime chưa đọc được đầy đủ, không mất dữ liệu.)
    match mgc_lockfile::detect_lockfile_version(&text)? {
        4 => {
            let v4 = mgc_lockfile::canonical::parse_v4_document(&text)?;
            let report = mgc_lockfile::policy::verify_v4_math(&v4)?;
            mgc_lockfile::policy::enforce_policy(
                &report,
                mgc_lockfile::policy::LockPolicyMode::Warn,
                &[],
            )?;
            if !report.signed {
                mgc_ui::warning(
                    "mgc.lock v4 digest is valid, but the lock is unsigned; signer trust was not evaluated",
                );
            } else {
                mgc_ui::warning(
                    "mgc.lock v4 signature math is valid; signer trust was not evaluated by this command",
                );
            }
            mgc_ui::info("mgc.lock v4 integrity is valid — no migration was needed.");
            Ok(())
        }
        1..=3 => {
            mgc_lockfile::ensure_lockfile_mutation_allowed(&lock_path)?;
            Err(crate::error::migrate_v4_runtime_unavailable())
        }
        other => Err(crate::error::migrate_unsupported_version(other)),
    }
}

#[cfg(test)]
#[path = "test/migrate.rs"]
mod tests;
