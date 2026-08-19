//! Lockfile format version migration (npm shrinkwrap.js:1003 model).
//!
//! - `version` in mg.lock is a *major* format version: bumping it means the
//!   on-disk structure changed incompatibly (npm refuses to read future
//!   lockfiles silently — it fails loud instead).
//! - `read_lockfile_checked` rejects checksum/signature tampering; here we
//!   additionally fail closed when the lockfile was written by a *newer* MG:
//!   never guess a structure we don't understand, never silently re-resolve.
//! - Older versions are migrated forward step by step when a migration fn
//!   exists; unknown future versions abort with a clear message.

use crate::Lockfile;

pub const SUPPORTED_VERSION: u32 = 1;

/// Current supported lockfile version. Bump this (MAJOR) only when the on-disk
/// structure of `Lockfile` changes incompatibly; all older versions need a
/// migration entry below to stay readable.
pub fn current_version() -> u32 {
    SUPPORTED_VERSION
}

/// Validate that we understand the lockfile, migrating older formats forward.
/// Fail-closed: a lockfile from a *newer* MG (or any version we cannot prove
/// we understand) is rejected instead of being silently ignored.
pub fn migrate(lock: &Lockfile) -> anyhow::Result<Lockfile> {
    if lock.version > SUPPORTED_VERSION {
        anyhow::bail!(
            "mg.lock version {} is newer than this version of mg (supports up to {}). \
             Upgrade mg to read this lockfile.",
            lock.version,
            SUPPORTED_VERSION
        );
    }
    if lock.version >= 1 {
        return Ok(lock.clone());
    }
    // version 0: only produced by buggy early drafts; reject rather than guess.
    anyhow::bail!(
        "mg.lock version 0 is not supported — regenerate the lockfile with 'mg install'."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_accepts_current_version() {
        let lock = Lockfile::new("web", "frontend");
        let out = migrate(&lock).unwrap();
        assert_eq!(out.version, 1);
    }

    #[test]
    fn migrate_rejects_future_version_fail_closed() {
        let mut lock = Lockfile::new("web", "frontend");
        lock.version = 99;
        let err = migrate(&lock).unwrap_err();
        assert!(err.to_string().contains("newer than this version"), "{err}");
    }

    #[test]
    fn migrate_rejects_version_zero() {
        let mut lock = Lockfile::new("web", "frontend");
        lock.version = 0;
        let err = migrate(&lock).unwrap_err();
        assert!(err.to_string().contains("version 0"), "{err}");
    }
}
