use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::StoreError;

pub fn conn_with_flags(path: &Path, flags: rusqlite::OpenFlags) -> Result<Connection, StoreError> {
    Ok(Connection::open_with_flags(path, flags)?)
}

pub fn detect_available_ram() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                2,
                &mut size as *mut _ as *mut std::ffi::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if result == 0 && size > 0 {
            return size;
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = val.parse::<u64>() {
                            let bytes = kb * 1024;
                            if bytes >= 512 * 1024 * 1024 {
                                return bytes;
                            }
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct MemoryStatusEx {
            dw_length: u32,
            dw_memory_load: u32,
            ull_total_phys: u64,
            ull_avail_phys: u64,
            ull_total_page_file: u64,
            ull_avail_page_file: u64,
            ull_total_virtual: u64,
            ull_avail_virtual: u64,
            ull_avail_extended_virtual: u64,
        }

        extern "system" {
            fn GlobalMemoryStatusEx(lp_buffer: *mut MemoryStatusEx) -> i32;
        }

        let mut state = MemoryStatusEx {
            dw_length: std::mem::size_of::<MemoryStatusEx>() as u32,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_extended_virtual: 0,
        };

        unsafe {
            if GlobalMemoryStatusEx(&mut state) != 0 && state.ull_total_phys > 0 {
                return state.ull_total_phys;
            }
        }
    }
    2 * 1024 * 1024 * 1024
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

    conn.pragma_update(None, "cache_size", &cache_size.to_string()).ok();
    conn.pragma_update(None, "mmap_size", &mmap_size.to_string()).ok();
    conn.pragma_update(None, "temp_store", "MEMORY").ok();
    conn.pragma_update(None, "busy_timeout", "5000").ok();
    Ok(())
}

pub fn health_check(conn: &Connection) -> Result<(), StoreError> {
    let result: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| StoreError::Database(format!("health check failed: {}", e)))?;
    if result != "ok" {
        return Err(StoreError::IntegrityCheck(result));
    }
    Ok(())
}

pub fn get_wal_size(path: &Path) -> i64 {
    let wal_path = append_filename_suffix(path, "-wal");
    let shm_path = append_filename_suffix(path, "-shm");
    let wal_size = std::fs::metadata(&wal_path).map(|m| m.len() as i64).unwrap_or(0);
    let shm_size = std::fs::metadata(&shm_path).map(|m| m.len() as i64).unwrap_or(0);
    wal_size + shm_size
}

pub fn append_filename_suffix(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut result = path.to_path_buf();
    result.set_file_name(format!("{}{}", name, suffix));
    result
}
