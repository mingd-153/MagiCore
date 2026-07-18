pub mod serialization;

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const LOCKFILE_NAME: &str = "mg.lock";
pub const LOCKFILE_CHECKSUM_NAME: &str = "mg.lock.sha256";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResolutionMeta {
    pub state: String,
    pub store: String,
    pub package_count: usize,
}

impl Default for ResolutionMeta {
    fn default() -> Self {
        Self {
            state: "pending".to_string(),
            store: "megagate".to_string(),
            package_count: 0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LockPackage {
    pub name: String,
    pub version: String,
    pub integrity: Option<String>,
    #[serde(default)]
    pub direct: bool,
    #[serde(default)]
    pub dev: bool,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceLock {
    pub path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub package_count: usize,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub core: String,
    pub mode: String,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub resolution: ResolutionMeta,
    #[serde(rename = "workspace", default)]
    pub workspaces: Vec<WorkspaceLock>,
    #[serde(rename = "package", default)]
    pub packages: Vec<LockPackage>,
    /// BLAKE3 HMAC signature of canonical lockfile content (optional, controlled by env).
    /// Format: `blake3:<hex-digest>` (keyed by MEGAGATE_LOCKFILE_KEY env var).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sig: Option<String>,
}

impl Lockfile {
    pub fn new(core: impl Into<String>, mode: impl Into<String>) -> Self {
        Self {
            version: 1,
            core: core.into(),
            mode: mode.into(),
            frameworks: vec![],
            resolution: ResolutionMeta::default(),
            workspaces: vec![],
            packages: vec![],
            sig: None,
        }
    }
}

pub fn lockfile_path(project_root: &Path) -> PathBuf {
    project_root.join(LOCKFILE_NAME)
}

pub fn lockfile_checksum_path(project_root: &Path) -> PathBuf {
    project_root.join(LOCKFILE_CHECKSUM_NAME)
}

pub fn lockfile_checksum(contents: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents);
    hex::encode(hasher.finalize())
}

pub fn write_lockfile_checksum(project_root: &Path, contents: &[u8]) -> anyhow::Result<()> {
    atomic_write(
        &lockfile_checksum_path(project_root),
        lockfile_checksum(contents).as_bytes(),
    )?;
    Ok(())
}

fn atomic_write(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, contents)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err.into())
        }
    }
}

pub fn read_lockfile_checked(project_root: &Path) -> anyhow::Result<Option<Lockfile>> {
    let path = lockfile_path(project_root);
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    let checksum_path = lockfile_checksum_path(project_root);
    if checksum_path.exists() {
        let expected = std::fs::read_to_string(&checksum_path)?;
        let actual = lockfile_checksum(contents.as_bytes());
        if actual.trim() != expected.trim() {
            anyhow::bail!("lockfile checksum mismatch - mg.lock may have been tampered with");
        }
    }

    let lock = serialization::from_toml::<Lockfile>(&contents)
        .map_err(|err| anyhow::anyhow!("failed to parse lockfile '{}': {}", path.display(), err))?;
    LockfileSigner::verify(&lock)?;
    Ok(Some(lock))
}

/// BLAKE3-keyed signing for mg.lock files.
/// Signing is opt-in; set MEGAGATE_LOCKFILE_KEY to a hex-encoded 32-byte secret.
pub struct LockfileSigner;

impl LockfileSigner {
    fn key() -> anyhow::Result<Option<[u8; 32]>> {
        let raw = match std::env::var("MEGAGATE_LOCKFILE_KEY") {
            Ok(raw) => raw,
            Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let bytes = hex::decode(raw.trim())
            .map_err(|err| anyhow::anyhow!("invalid MEGAGATE_LOCKFILE_KEY hex: {err}"))?;
        if bytes.len() != 32 {
            anyhow::bail!(
                "invalid MEGAGATE_LOCKFILE_KEY length: expected 32 bytes, got {}",
                bytes.len()
            );
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(Some(key))
    }

    /// Compute a canonical (stable) representation of the lockfile without the `sig` field.
    fn canonical(lock: &Lockfile) -> anyhow::Result<String> {
        let mut tmp = lock.clone();
        tmp.sig = None;
        // Sort packages deterministically
        tmp.packages
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
        Ok(serialization::to_toml(&tmp)?)
    }

    /// Sign the lockfile in-place. No-op if MEGAGATE_LOCKFILE_KEY is not set.
    pub fn sign(lock: &mut Lockfile) -> anyhow::Result<()> {
        let Some(key) = Self::key()? else {
            return Ok(());
        };
        let canonical = Self::canonical(lock)?;
        let digest = blake3::keyed_hash(&key, canonical.as_bytes());
        lock.sig = Some(format!("blake3:{}", hex::encode(digest.as_bytes())));
        Ok(())
    }

    /// Verify the lockfile signature. Returns Ok(true) when signed+valid, Ok(false) when unsigned.
    /// Returns an error when the signature is present but invalid.
    pub fn verify(lock: &Lockfile) -> anyhow::Result<bool> {
        let key = Self::key()?;
        let Some(ref sig_str) = lock.sig else {
            return Ok(false);
        };
        let Some(key) = key else {
            anyhow::bail!("lockfile is signed but MEGAGATE_LOCKFILE_KEY is not set");
        };
        let hex_digest = sig_str.strip_prefix("blake3:").ok_or_else(|| {
            anyhow::anyhow!("unknown lockfile signature algorithm in '{}'", sig_str)
        })?;
        let expected = hex::decode(hex_digest)
            .map_err(|e| anyhow::anyhow!("invalid hex in lockfile signature: {e}"))?;
        let canonical = Self::canonical(lock)?;
        let actual = blake3::keyed_hash(&key, canonical.as_bytes());
        if actual.as_bytes() == expected.as_slice() {
            Ok(true)
        } else {
            Err(anyhow::anyhow!(
                "lockfile signature mismatch — possible tampering detected"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn test_resolution_meta_default() {
        let meta = ResolutionMeta::default();
        assert_eq!(meta.state, "pending");
        assert_eq!(meta.store, "megagate");
        assert_eq!(meta.package_count, 0);
    }

    #[test]
    fn test_lockfile_new_defaults() {
        let lock = Lockfile::new("test-core", "test-mode");
        assert_eq!(lock.version, 1);
        assert_eq!(lock.core, "test-core");
        assert_eq!(lock.mode, "test-mode");
        assert!(lock.frameworks.is_empty());
        assert_eq!(lock.resolution.state, "pending");
        assert!(lock.workspaces.is_empty());
        assert!(lock.packages.is_empty());
    }

    #[test]
    fn test_lockfile_checksum_is_sha256_hex() {
        assert_eq!(
            lockfile_checksum(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_read_lockfile_checked_rejects_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Lockfile::new("web", "frontend");
        std::fs::write(
            lockfile_path(dir.path()),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();
        std::fs::write(lockfile_checksum_path(dir.path()), "bad").unwrap();

        let err = read_lockfile_checked(dir.path()).unwrap_err();

        assert!(
            err.to_string().contains("lockfile checksum mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_sign_rejects_invalid_lockfile_key() {
        let _guard = env_lock();
        std::env::set_var("MEGAGATE_LOCKFILE_KEY", "not-hex");
        let mut lock = Lockfile::new("web", "frontend");

        let err = LockfileSigner::sign(&mut lock).unwrap_err();

        std::env::remove_var("MEGAGATE_LOCKFILE_KEY");
        assert!(
            err.to_string().contains("invalid MEGAGATE_LOCKFILE_KEY"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_verify_rejects_signed_lock_without_key() {
        let _guard = env_lock();
        std::env::remove_var("MEGAGATE_LOCKFILE_KEY");
        let mut lock = Lockfile::new("web", "frontend");
        lock.sig = Some("blake3:00".into());

        let err = LockfileSigner::verify(&lock).unwrap_err();

        assert!(
            err.to_string()
                .contains("lockfile is signed but MEGAGATE_LOCKFILE_KEY is not set"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_sign_and_verify_with_valid_key() {
        let _guard = env_lock();
        std::env::set_var(
            "MEGAGATE_LOCKFILE_KEY",
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        );
        let mut lock = Lockfile::new("web", "frontend");

        LockfileSigner::sign(&mut lock).unwrap();
        assert!(LockfileSigner::verify(&lock).unwrap());

        std::env::remove_var("MEGAGATE_LOCKFILE_KEY");
    }

    #[test]
    fn test_json_roundtrip() {
        let mut lock = Lockfile::new("backend", "api");
        lock.frameworks = vec!["actix-web".to_string()];
        lock.resolution = ResolutionMeta {
            state: "locked".to_string(),
            store: "megagate".to_string(),
            package_count: 2,
        };
        lock.packages.push(LockPackage {
            name: "serde".to_string(),
            version: "1.0.200".to_string(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
        });
        lock.packages.push(LockPackage {
            name: "tokio".to_string(),
            version: "1.38.0".to_string(),
            integrity: Some("sha256-abc".to_string()),
            direct: false,
            dev: true,
            dependencies: vec!["bytes@1.6.0".to_string()],
        });

        let json = serialization::to_json(&lock).unwrap();
        let parsed: Lockfile = serialization::from_json(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.core, "backend");
        assert_eq!(parsed.mode, "api");
        assert_eq!(parsed.frameworks, vec!["actix-web"]);
        assert_eq!(parsed.resolution.package_count, 2);
        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.packages[0].name, "serde");
        assert_eq!(parsed.packages[0].integrity, None);
        assert!(parsed.packages[0].direct);
        assert!(!parsed.packages[0].dev);
        assert!(parsed.packages[0].dependencies.is_empty());
        assert_eq!(parsed.packages[1].name, "tokio");
        assert_eq!(parsed.packages[1].integrity, Some("sha256-abc".to_string()));
        assert!(!parsed.packages[1].direct);
        assert!(parsed.packages[1].dev);
    }

    #[test]
    fn test_json_empty_lists() {
        let lock = Lockfile::new("cli", "rust");
        let json = serialization::to_json(&lock).unwrap();
        let parsed: Lockfile = serialization::from_json(&json).unwrap();
        assert!(parsed.workspaces.is_empty());
        assert!(parsed.packages.is_empty());
        assert!(parsed.frameworks.is_empty());
    }

    #[test]
    fn test_toml_roundtrip() {
        let mut lock = Lockfile::new("web", "frontend");
        lock.frameworks = vec!["react-vite".to_string()];
        lock.resolution = ResolutionMeta {
            state: "locked".to_string(),
            store: "megagate".to_string(),
            package_count: 1,
        };
        lock.packages.push(LockPackage {
            name: "tailwindcss".to_string(),
            version: "4.3.2".to_string(),
            integrity: Some("sha512-test".to_string()),
            direct: true,
            dev: false,
            dependencies: vec!["@tailwindcss/node@4.3.2".to_string()],
        });

        let toml = serialization::to_toml(&lock).unwrap();
        let parsed: Lockfile = serialization::from_toml(&toml).unwrap();

        assert_eq!(parsed.core, "web");
        assert_eq!(parsed.mode, "frontend");
        assert_eq!(parsed.frameworks, vec!["react-vite"]);
        assert_eq!(parsed.resolution.package_count, 1);
        assert!(parsed.workspaces.is_empty());
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].name, "tailwindcss");
        assert_eq!(parsed.packages[0].version, "4.3.2");
        assert_eq!(
            parsed.packages[0].integrity,
            Some("sha512-test".to_string())
        );
        assert!(parsed.packages[0].direct);
        assert!(!parsed.packages[0].dev);
    }

    #[test]
    fn test_toml_empty_roundtrip() {
        let lock = Lockfile::new("empty", "test");
        let toml = serialization::to_toml(&lock).unwrap();
        let parsed: Lockfile = serialization::from_toml(&toml).unwrap();
        assert_eq!(parsed.core, "empty");
        assert_eq!(parsed.mode, "test");
        assert!(parsed.packages.is_empty());
        assert!(parsed.workspaces.is_empty());
    }

    #[test]
    fn test_toml_with_workspaces() {
        let mut lock = Lockfile::new("monorepo", "fullstack");
        lock.workspaces.push(WorkspaceLock {
            path: "packages/web".to_string(),
            name: "web".to_string(),
            mode: "frontend".to_string(),
            frameworks: vec!["react".to_string()],
            package_count: 5,
        });
        lock.workspaces.push(WorkspaceLock {
            path: "packages/api".to_string(),
            name: "api".to_string(),
            mode: "backend".to_string(),
            frameworks: vec![],
            package_count: 3,
        });

        let toml = serialization::to_toml(&lock).unwrap();
        let parsed: Lockfile = serialization::from_toml(&toml).unwrap();
        assert_eq!(parsed.workspaces.len(), 2);
        assert_eq!(parsed.workspaces[0].path, "packages/web");
        assert_eq!(parsed.workspaces[0].name, "web");
        assert_eq!(parsed.workspaces[0].mode, "frontend");
        assert_eq!(parsed.workspaces[0].frameworks, vec!["react"]);
        assert_eq!(parsed.workspaces[0].package_count, 5);
        assert_eq!(parsed.workspaces[1].path, "packages/api");
        assert_eq!(parsed.workspaces[1].name, "api");
        assert_eq!(parsed.workspaces[1].package_count, 3);
    }

    #[test]
    fn test_workspace_lock_defaults() {
        let ws = WorkspaceLock {
            path: "libs/util".to_string(),
            ..Default::default()
        };
        assert_eq!(ws.path, "libs/util");
        assert!(ws.name.is_empty());
        assert!(ws.mode.is_empty());
        assert!(ws.frameworks.is_empty());
        assert_eq!(ws.package_count, 0);
    }

    #[test]
    fn test_lock_package_defaults() {
        let pkg = LockPackage {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
        };
        assert_eq!(pkg.integrity, None);
        assert!(!pkg.direct);
        assert!(!pkg.dev);
        assert!(pkg.dependencies.is_empty());
    }

    #[test]
    fn test_malformed_json() {
        let result = serialization::from_json::<Lockfile>("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_toml() {
        let result = serialization::from_toml::<Lockfile>("[[[invalid toml]]]");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_fields_toml() {
        let minimal = r#"version = 1
core = "minimal"
mode = "test""#;
        let parsed: Lockfile = serialization::from_toml(minimal).unwrap();
        assert_eq!(parsed.core, "minimal");
        assert_eq!(parsed.mode, "test");
        assert_eq!(parsed.version, 1);
        assert!(parsed.frameworks.is_empty());
        assert!(parsed.workspaces.is_empty());
        assert!(parsed.packages.is_empty());
        assert_eq!(parsed.resolution.state, "pending");
    }

    #[test]
    fn test_missing_fields_json() {
        let minimal = r#"{"version": 1, "core": "minimal", "mode": "test"}"#;
        let parsed: Lockfile = serialization::from_json(minimal).unwrap();
        assert_eq!(parsed.core, "minimal");
        assert_eq!(parsed.mode, "test");
        assert_eq!(parsed.version, 1);
        assert!(parsed.frameworks.is_empty());
        assert!(parsed.workspaces.is_empty());
        assert!(parsed.packages.is_empty());
        assert_eq!(parsed.resolution.state, "pending");
    }

    #[test]
    fn test_json_preserves_integrity_none() {
        let pkg = LockPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
        };
        let json = serialization::to_json(&pkg).unwrap();
        let parsed: LockPackage = serialization::from_json(&json).unwrap();
        assert_eq!(parsed.integrity, None);
    }
}
