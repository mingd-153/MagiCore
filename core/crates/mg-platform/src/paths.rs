//! mg-platform/paths.rs — Standard paths (RULE §12: no hardcoded paths)
//! (Đường dẫn chuẩn: store/cache/logs/locks/dlx/advisories/patches/quarantine/registry + .megagate/lock-signatures.json)

use std::path::{Path, PathBuf};

/// Standard subdirectories under project root (.megagate/)
pub struct ProjectPaths {
    pub root: PathBuf,
    pub store: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    pub locks: PathBuf,
    pub dlx: PathBuf,
    pub advisories: PathBuf,
    pub patches: PathBuf,
    pub quarantine: PathBuf,
    pub registry: PathBuf,
    pub lock_signatures: PathBuf, // sidecar .megagate/lock-signatures.json (ref: 16 §4, phản biện v2 #5)
}

impl ProjectPaths {
    /// Compute all standard paths from project root.
    pub fn from_root(root: &Path) -> Self {
        let mg_dir = root.join(".megagate");
        Self {
            root: root.to_path_buf(),
            store: mg_dir.join("store"),
            cache: mg_dir.join("cache"),
            logs: mg_dir.join("logs"),
            locks: mg_dir.join("locks"),
            dlx: mg_dir.join("dlx"),
            advisories: mg_dir.join("advisories"),
            patches: mg_dir.join("patches"),
            quarantine: mg_dir.join("quarantine"),
            registry: mg_dir.join("registry"),
            lock_signatures: mg_dir.join("lock-signatures.json"),
        }
    }

    /// Ensure all directories exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.store)?;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::create_dir_all(&self.logs)?;
        std::fs::create_dir_all(&self.locks)?;
        std::fs::create_dir_all(&self.dlx)?;
        std::fs::create_dir_all(&self.advisories)?;
        std::fs::create_dir_all(&self.patches)?;
        std::fs::create_dir_all(&self.quarantine)?;
        std::fs::create_dir_all(&self.registry)?;
        Ok(())
    }

    /// Path to patches directory (~/.megagate/patches/ or <project>/.megagate/patches/)
    pub fn patches_dir(&self) -> &Path {
        &self.patches
    }
}

/// User-global paths (~/.megagate/)
pub struct GlobalPaths {
    pub root: PathBuf,
    pub store: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    pub locks: PathBuf,
    pub dlx: PathBuf,
    pub advisories: PathBuf,
    pub patches: PathBuf,
    pub quarantine: PathBuf,
    pub registry: PathBuf,
    pub lock_signatures: PathBuf,
}

impl GlobalPaths {
    pub fn new() -> std::io::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
        let mg_dir = home.join(".megagate");
        Ok(Self {
            root: mg_dir.clone(),
            store: mg_dir.join("store"),
            cache: mg_dir.join("cache"),
            logs: mg_dir.join("logs"),
            locks: mg_dir.join("locks"),
            dlx: mg_dir.join("dlx"),
            advisories: mg_dir.join("advisories"),
            patches: mg_dir.join("patches"),
            quarantine: mg_dir.join("quarantine"),
            registry: mg_dir.join("registry"),
            lock_signatures: mg_dir.join("lock-signatures.json"),
        })
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.store)?;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::create_dir_all(&self.logs)?;
        std::fs::create_dir_all(&self.locks)?;
        std::fs::create_dir_all(&self.dlx)?;
        std::fs::create_dir_all(&self.advisories)?;
        std::fs::create_dir_all(&self.patches)?;
        std::fs::create_dir_all(&self.quarantine)?;
        std::fs::create_dir_all(&self.registry)?;
        Ok(())
    }

    pub fn patches_dir(&self) -> &Path {
        &self.patches
    }
}

/// Detect project root (walk up for markers) — shared with mg-config/paths.rs
pub fn find_project_root(from: &Path) -> Option<PathBuf> {
    let mut current = from.to_path_buf();
    loop {
        if current.join("mg.toml").exists()
            || current.join("mg.lock").exists()
            || current.join("package.json").exists()
            || current.join("Cargo.toml").exists()
            || current.join("pyproject.toml").exists()
            || current.join(".git").exists()
        {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}
