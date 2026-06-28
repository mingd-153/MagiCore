use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

impl SqliteStore {
    pub fn audit(&self) -> Result<AuditReport, StoreError> {
        let mut warnings = Vec::new();
        let now = SystemTime::now();
        let now_secs = now.duration_since(UNIX_EPOCH).unwrap().as_secs();

        let quick_check_ok = match self.health_check() {
            Ok(_) => true,
            Err(StoreError::IntegrityCheck(msg)) => {
                warnings.push(format!("quick check failed: {}", msg));
                false
            }
            Err(e) => {
                warnings.push(format!("health check error: {}", e));
                false
            }
        };

        let deep_check_ok = if quick_check_ok {
            match self.deep_integrity_check() {
                Ok(_) => true,
                Err(StoreError::IntegrityCheck(msg)) => {
                    warnings.push(format!("deep integrity check failed: {}", msg));
                    false
                }
                Err(e) => {
                    warnings.push(format!("deep integrity check error: {}", e));
                    false
                }
            }
        } else {
            warnings.push("skipping deep integrity check (quick check failed)".to_string());
            false
        };

        let integrity_ok = quick_check_ok && deep_check_ok;

        let (permissions_ok, perm_warnings, last_audit_str, stale_hours) =
            self.check_permissions_inner(now_secs);

        warnings.extend(perm_warnings);

        let stale = stale_hours > STALE_WARNING_HOURS as f64;
        if stale {
            warnings.push(format!(
                "store not audited in {:.0}h (threshold: {}h)",
                stale_hours, STALE_WARNING_HOURS
            ));
        }

        let (db_size_mb, wal_size_kb, cache_entries) = self.get_store_stats()?;
        let ram_gb = detect_available_ram() / (1024 * 1024 * 1024);

        let passed = integrity_ok && permissions_ok && !stale;

        self.set_kv("audit_last_run", &now_secs.to_le_bytes()).ok();

        Ok(AuditReport {
            passed,
            integrity_ok,
            permissions_ok,
            stale_warning: stale,
            stale_hours,
            last_audit: last_audit_str,
            warnings,
            db_size_mb,
            wal_size_kb,
            cache_entries,
            detected_ram_gb: ram_gb,
        })
    }

    pub fn check_permissions(&self) -> Result<Vec<String>, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (_, warnings, _, _) = self.check_permissions_inner(now);
        Ok(warnings)
    }

    pub fn snapshot_permissions(&self) -> Result<(), StoreError> {
        let entries = self.collect_permissions();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let snapshot = PermissionSnapshot {
            files: entries,
            recorded_at: now,
        };
        let json = serde_json::to_string(&snapshot)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        self.set_kv("permission_snapshot", json.as_bytes())?;
        self.set_kv("permission_snapshot_time", &now.to_le_bytes())?;
        Ok(())
    }

    fn check_permissions_inner(&self, now_secs: u64) -> (bool, Vec<String>, String, f64) {
        let mut warnings = Vec::new();

        let last_snapshot = self
            .get_kv("permission_snapshot_time")
            .ok()
            .flatten()
            .and_then(|v| {
                let arr: [u8; 8] = v.as_slice().try_into().ok()?;
                Some(u64::from_le_bytes(arr))
            });

        let last_audit_str = match last_snapshot {
            Some(ts) => {
                let hours = (now_secs.saturating_sub(ts)) as f64 / 3600.0;
                format!("{:.1}h ago", hours)
            }
            None => "never".to_string(),
        };

        let stale_hours = match last_snapshot {
            Some(ts) => (now_secs.saturating_sub(ts)) as f64 / 3600.0,
            None => f64::MAX,
        };

        let snapshot_json = match self.get_kv("permission_snapshot") {
            Ok(Some(data)) => data,
            _ => {
                self.snapshot_permissions().ok();
                return (true, vec![], "just now (initial snapshot)".to_string(), 0.0);
            }
        };

        let snapshot: PermissionSnapshot = match serde_json::from_slice(&snapshot_json) {
            Ok(s) => s,
            Err(_) => {
                return (
                    false,
                    vec!["corrupted permission snapshot".to_string()],
                    "unknown".to_string(),
                    stale_hours,
                );
            }
        };

        let current = self.collect_permissions();
        let mut ok = true;

        for stored in &snapshot.files {
            match current.iter().find(|c| c.path == stored.path) {
                Some(curr) => {
                    if curr.mode != stored.mode {
                        warnings.push(format!(
                            "permission changed: {} was {:o} now {:o}",
                            stored.path, stored.mode, curr.mode
                        ));
                        ok = false;
                    }
                    if curr.modified_at > stored.modified_at {
                        warnings.push(format!("file modified: {} (mtime changed)", stored.path));
                        ok = false;
                    }
                }
                None => {
                    warnings.push(format!("file removed: {}", stored.path));
                    ok = false;
                }
            }
        }

        for curr in &current {
            if !snapshot.files.iter().any(|s| s.path == curr.path) {
                warnings.push(format!("new file detected: {}", curr.path));
            }
        }

        (ok, warnings, last_audit_str, stale_hours)
    }

    fn collect_permissions(&self) -> Vec<FilePermissionEntry> {
        let mut entries = Vec::new();
        let paths = [
            self.path.clone(),
            append_filename_suffix(&self.path, "-wal"),
            append_filename_suffix(&self.path, "-shm"),
        ];

        for p in &paths {
            if let Ok(meta) = fs::metadata(p) {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    meta.permissions().mode()
                };
                #[cfg(not(unix))]
                let mode = 0;
                entries.push(FilePermissionEntry {
                    path: p.to_string_lossy().to_string(),
                    mode,
                    size: meta.len(),
                    modified_at: modified,
                });
            }
        }
        entries
    }
}
