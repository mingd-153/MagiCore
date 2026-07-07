use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lru::LruCache;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::index;

mod audit;
mod generation;
mod kv;
mod lifecycle;
mod schema;
mod store;
#[cfg(test)]
mod tests;
mod util;

pub(crate) use index::{AuditReport, PackageInfo, StoreError, StoreIndex};
pub(crate) use schema::{create_tables, migrate_schema, row_to_package};
pub(crate) use util::*;

pub struct SqliteStore {
    conn: Mutex<Connection>,
    cache: Mutex<LruCache<String, PackageInfo>>,
    path: PathBuf,
    readonly: bool,
    generation: Mutex<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PermissionSnapshot {
    files: Vec<FilePermissionEntry>,
    recorded_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FilePermissionEntry {
    path: String,
    mode: u32,
    size: u64,
    modified_at: u64,
}

const STALE_WARNING_HOURS: u64 = 24;

impl SqliteStore {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_readonly(&self) -> bool {
        self.readonly
    }
}

const LRU_CACHE_SIZE: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(1000) {
    Some(v) => v,
    None => unreachable!(),
};

impl Clone for SqliteStore {
    fn clone(&self) -> Self {
        let conn = Connection::open(&self.path).unwrap_or_else(|e| {
            panic!("failed to clone SQLite connection at {}: {}", self.path.display(), e)
        });
        Self {
            conn: Mutex::new(conn),
            cache: Mutex::new(LruCache::new(LRU_CACHE_SIZE)),
            path: self.path.clone(),
            readonly: self.readonly,
            generation: Mutex::new(0),
        }
    }
}
