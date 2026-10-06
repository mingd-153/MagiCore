//! `mgc migrate` — explicit lockfile migrations (V1.2 §16.4).
//! Migration lockfile tường minh.
//!
//! v4 writes are allowed only when fully lossless: install/add (via the
//! shared v4 loader) consume v4 without projecting away identity, and
//! `migrate_v3_to_v4` reports zero warnings. Any dropped edge, peer,
//! source, or installer SRI keeps the existing lock untouched.
//! Existing v4 files are accepted as a no-op.

use anyhow::Result;
use clap::Subcommand;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Subcommand, Debug, Clone)]
pub enum MigrateCmd {
    /// Migrate mgc.lock (v4 writes require a fully lossless migration)
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
            // Emit v4 only when the migration is fully lossless: any
            // dropped edge/peer/source or SRI-less pin keeps the old lock
            // untouched (fail-closed — a partial v4 could never drive
            // install without silent verification gaps).
            // (Chỉ ghi v4 khi migration lossless hoàn toàn: hao hụt nào
            // cũng giữ nguyên lock cũ.)
            let legacy = mgc_lockfile::parser::parse_lockfile(&text)
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            let (mut v4, warnings) = mgc_lockfile::migrate_v3_to_v4(legacy)
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            if !warnings.is_empty() {
                return Err(crate::error::migrate_v4_lossy(&warnings));
            }
            v4.metadata.generated_at = chrono::Utc::now().to_rfc3339();
            v4.metadata.generator = format!("mgc/{}", env!("CARGO_PKG_VERSION"));
            v4.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&v4.payload());
            v4.metadata.signature = None;
            let document = mgc_lockfile::canonical::write_v4_document(&v4)
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            // Re-parse what will hit disk: the emitted text must round-trip
            // into the identical document (no serializer drift).
            // (Parse lại đúng text sẽ ghi đĩa: phải round-trip y hệt.)
            let round_trip = mgc_lockfile::canonical::parse_v4_document(&document)
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            if round_trip != v4 {
                return Err(crate::error::migrate_v4_roundtrip_mismatch());
            }
            mgc_lockfile::atomic::atomic_write_locked(
                &guard,
                &lock_path,
                document.as_bytes(),
                std::time::Duration::from_secs(60),
            )
            .map_err(|error| anyhow::anyhow!("{error}"))?;
            mgc_ui::warning(
                "migrated mgc.lock to schema v4 unsigned: the previous lock signature (if any) no longer applies — run `mgc trust sign` to sign the new lock",
            );
            mgc_ui::info(
                "migrated mgc.lock to schema v4 (lossless — no edge, peer, source, or SRI dropped).",
            );
            Ok(())
        }
        other => Err(crate::error::migrate_unsupported_version(other)),
    }
}

#[cfg(test)]
#[path = "test/migrate.rs"]
mod tests;
