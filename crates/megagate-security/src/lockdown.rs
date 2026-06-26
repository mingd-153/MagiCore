use megagate_types::error::{MegagateError, Result};
use megagate_types::package::LockedPackage;
use std::path::Path;

pub struct LockdownManager;

impl LockdownManager {
    pub fn new() -> Self {
        Self
    }

    pub fn check(&self, pkg: &LockedPackage, extract_path: &Path) -> Result<()> {
        self.check_native_addons(extract_path)?;
        self.check_eval_usage(extract_path)?;
        self.check_side_effects(pkg)?;
        Ok(())
    }

    fn check_native_addons(&self, path: &Path) -> Result<()> {
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.map_err(|e| MegagateError::SecurityViolation(e.to_string()))?;
            if entry.file_name().to_string_lossy().ends_with(".node") {
                return Err(MegagateError::SecurityViolation(
                    format!("Native addon found: {}", entry.path().display())
                ));
            }
        }
        Ok(())
    }

    fn check_eval_usage(&self, path: &Path) -> Result<()> {
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.map_err(|e| MegagateError::SecurityViolation(e.to_string()))?;
            if entry.path().extension().map(|e| e == "js").unwrap_or(false) {
                let content = std::fs::read_to_string(entry.path())
                    .map_err(|e| MegagateError::IoError(e.to_string()))?;
                if content.contains("eval(") || content.contains("Function(") || content.contains("new Function(") {
                    return Err(MegagateError::SecurityViolation(
                        format!("eval/Function constructor found in: {}", entry.path().display())
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_side_effects(&self, pkg: &LockedPackage) -> Result<()> {
        if pkg.dependencies.values().any(|v| v.contains("sideEffects")) {
            return Err(MegagateError::SecurityViolation(
                "Package declares sideEffects without explicit false".to_string()
            ));
        }
        Ok(())
    }
}

impl Default for LockdownManager {
    fn default() -> Self {
        Self::new()
    }
}