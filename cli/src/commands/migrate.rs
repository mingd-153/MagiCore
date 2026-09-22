//! `mgc migrate` — explicit lockfile migrations (V1.2 §16.4).
//! Migration lockfile tường minh.
//!
//! v4 is NEVER an automatic target: `mgc migrate lock --to v4` is the
//! single explicit entry point. Every lossy decision surfaces as a
//! warning (the migration never invents hashes, versions, or sources).

use anyhow::Result;
use clap::Subcommand;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Subcommand, Debug, Clone)]
pub enum MigrateCmd {
    /// Migrate mgc.lock to a newer schema (only `--to v4` exists)
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
    let lock_path = root.join("mgc.lock");
    if !lock_path.exists() {
        return Err(crate::error::migrate_no_lockfile(&root));
    }
    let text = std::fs::read_to_string(&lock_path)?;
    // v1/v2 chain through v3 in memory inside this ONE explicit
    // invocation (no silent auto-upgrade anywhere else).
    // (v1/v2 nối qua v3 trong bộ nhớ trong ĐÚNG một lần gọi tường minh
    // này.)
    let v3 = match mgc_lockfile::detect_lockfile_version(&text)? {
        4 => {
            mgc_ui::info("mgc.lock is already schema v4 — nothing to do.");
            return Ok(());
        }
        3 => mgc_lockfile::parser::parse_lockfile(&text)?,
        1 | 2 => mgc_lockfile::auto_upgrade_lockfile(&text)?,
        other => {
            return Err(crate::error::migrate_unsupported_version(other));
        }
    };
    let (mut v4, warnings) = mgc_lockfile::migrate_v3_to_v4(v3)?;
    for warning in &warnings {
        mgc_ui::warning(&format!("migrate: {warning}"));
    }
    v4.metadata.generated_at = chrono::Utc::now().to_rfc3339();
    v4.metadata.generator = format!("mgc/{} (migrated v3->v4)", env!("CARGO_PKG_VERSION"));
    v4.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&v4.payload());
    let bytes = mgc_lockfile::canonical::write_v4_document(&v4)?;
    let guard = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        &root,
        Duration::from_millis(acquire_timeout_ms(&root)),
    )?;
    // Mutation gateway parity: never migrate over an unrestored mutation
    // journal (P0) — recover first via remove/install, then migrate.
    // (Không migrate đè lên journal chưa phục hồi.)
    crate::commands::core::shared::ensure_no_pending_remove_journal(&root, &guard)?;
    mgc_lockfile::atomic::atomic_write_locked(
        &guard,
        &lock_path,
        bytes.as_bytes(),
        Duration::from_secs(60),
    )?;
    mgc_ui::success(&format!(
        "migrated mgc.lock v3 -> v4 (digest {}, {} warning(s))",
        v4.metadata.lockfile_hash,
        warnings.len()
    ));
    Ok(())
}

#[cfg(test)]
#[path = "test/migrate.rs"]
mod tests;
