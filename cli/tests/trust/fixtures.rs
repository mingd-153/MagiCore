#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Shared trust test fixtures.
//! Fixture chung cho test trust.

use mgc_lockfile::{write_lockfile, Lockfile, Package};
use std::path::{Path, PathBuf};

pub fn write_empty_lockfile(path: &Path) {
    write_lockfile(&Lockfile::new(), path).unwrap();
}

pub fn write_lockfile_with_package(path: &Path, name: &str, version: &str, integrity: &str) {
    let mut lockfile = Lockfile::new();
    lockfile.add_package(package(name, version, integrity));
    write_lockfile(&lockfile, path).unwrap();
}

pub fn write_lockfile_with_packages(path: &Path, packages: &[(&str, &str, &str)]) {
    let mut lockfile = Lockfile::new();
    for (name, version, integrity) in packages {
        lockfile.add_package(package(name, version, integrity));
    }
    write_lockfile(&lockfile, path).unwrap();
}

pub fn test_keyring_path(label: &str) -> PathBuf {
    let home = dirs::home_dir().expect("home directory should be available for keyring tests");
    home.join(".magicore").join("test-keyrings").join(format!(
        "{}-{}-{}.json",
        label,
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}

pub fn cleanup_keyring(path: &Path) {
    let _ = std::fs::remove_file(path);
    let backup_path = path.with_extension("json.bak");
    let _ = std::fs::remove_file(backup_path);
}

fn package(name: &str, version: &str, integrity: &str) -> Package {
    Package::new(
        name.to_string(),
        version.to_string(),
        format!("https://registry.example.com/{name}/-/{name}-{version}.tgz"),
        integrity.to_string(),
    )
}
