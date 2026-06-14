use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PackageRef {
    pub name: String,
    pub version: String,
    pub source: String,
    pub integrity: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub dep_type: String, // runtime, dev, optional
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LockFile {
    pub packages: Vec<PackageRef>,
    pub edges: Vec<DependencyEdge>,
    pub generated_at: String,
}

pub fn load_lock(dir: &str) -> Result<LockFile> {
    let path = Path::new(dir).join("mega-lock.json");
    if path.exists() {
        let data = fs::read_to_string(&path)?;
        let lock: LockFile = serde_json::from_str(&data)?;
        Ok(lock)
    } else {
        Ok(LockFile::default())
    }
}

pub fn save_lock(dir: &str, lock: &LockFile) -> Result<()> {
    let path = Path::new(dir).join("mega-lock.json");
    let json = serde_json::to_string_pretty(lock)?;
    fs::write(path, json)?;
    Ok(())
}
