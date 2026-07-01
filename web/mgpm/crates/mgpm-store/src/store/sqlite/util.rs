use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sysinfo::System;

use super::StoreError;

pub fn conn_with_flags(path: &Path, flags: rusqlite::OpenFlags) -> Result<Connection, StoreError> {
    Ok(Connection::open_with_flags(path, flags)?)
}

pub fn detect_available_ram() -> u64 {
    let mut sys = System::new_all();
    sys.refresh_memory();
    let total = sys.total_memory();
    if total >= 512 * 1024 * 1024 {
        total
    } else {
        2 * 1024 * 1024 * 1024
    }
}

pub fn adaptive_cache_size(ram: u64) -> i64 {
    if ram < 512 * 1024 * 1024 {
        -2000
    } else if ram < 1024 * 1024 * 1024 {
        -8000
    } else if ram < 2 * 1024 * 1024 * 1024 {
        -32000
    } else if ram < 4 * 1024 * 1024 * 1024 {
        -64000
    } else if ram < 8 * 1024 * 1024 * 1024 {
        -128000
    } else if ram < 16 * 1024 * 1024 * 1024 {
        -256000
    } else {
        -512000
    }
}

pub fn adaptive_mmap_size(ram: u64) -> i64 {
    if ram < 1024 * 1024 * 1024 {
        0
    } else if ram < 4 * 1024 * 1024 * 1024 {
        16 * 1024 * 1024
    } else if ram < 8 * 1024 * 1024 * 1024 {
        64 * 1024 * 1024
    } else if ram < 16 * 1024 * 1024 * 1024 {
        128 * 1024 * 1024
    } else {
        256 * 1024 * 1024
    }
}

pub fn adaptive_lru_size(ram: u64) -> usize {
    let entries = (ram / (1024 * 1024)).saturating_mul(50);
    entries.clamp(1000, 100_000) as usize
}

pub fn apply_pragmas(conn: &Connection, ram: u64) -> Result<(), StoreError> {
    let cache_size = adaptive_cache_size(ram);
    let mmap_size = adaptive_mmap_size(ram);

    let sqls = [
        ("journal_mode", "WAL"),
        ("synchronous", "NORMAL"),
        ("cache_size", &cache_size.to_string()),
        ("mmap_size", &mmap_size.to_string()),
        ("temp_store", "MEMORY"),
        ("wal_autocheckpoint", "10000"),
        ("busy_timeout", "5000"),
        ("trusted_schema", "OFF"),
    ];
    for (name, value) in sqls {
        conn.pragma_update(None, name, value).ok();
    }
    Ok(())
}

pub fn apply_pragmas_readonly(conn: &Connection, ram: u64) -> Result<(), StoreError> {
    let cache_size = adaptive_cache_size(ram);
    let mmap_size = adaptive_mmap_size(ram);

    conn.pragma_update(None, "cache_size", cache_size.to_string())
        .ok();
    conn.pragma_update(None, "mmap_size", mmap_size.to_string())
        .ok();
    conn.pragma_update(None, "temp_store", "MEMORY").ok();
    conn.pragma_update(None, "busy_timeout", "5000").ok();
    Ok(())
}

pub fn health_check(conn: &Connection) -> Result<(), StoreError> {
    // quick_check: fast, checks b-tree structure
    let result: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| StoreError::Database(format!("health check failed: {}", e)))?;
    if result != "ok" {
        return Err(StoreError::IntegrityCheck(result));
    }
    Ok(())
}

#[allow(dead_code)]
pub fn deep_integrity_check(conn: &Connection) -> Result<(), StoreError> {
    // integrity_check: full verification including page checksums, freelist, etc.
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| StoreError::Database(format!("deep integrity check failed: {}", e)))?;
    if result != "ok" {
        return Err(StoreError::IntegrityCheck(result));
    }
    Ok(())
}

pub fn get_wal_size(path: &Path) -> i64 {
    let wal_path = append_filename_suffix(path, "-wal");
    let shm_path = append_filename_suffix(path, "-shm");
    let wal_size = std::fs::metadata(&wal_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let shm_size = std::fs::metadata(&shm_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    wal_size + shm_size
}

pub fn append_filename_suffix(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("db");
    let mut result = path.to_path_buf();
    result.set_file_name(format!("{}{}", name, suffix));
    result
}
