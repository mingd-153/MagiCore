//! MagiCore-owned ecosystem install roots — gốc cài package theo ecosystem do MagiCore quản lý.
//!
//! Native installers resolve and verify artifacts, import their bytes into
//! the CAS, then materialize ecosystem-compatible layouts under these roots.
//! Installer native tự resolve/xác minh artifact, import byte vào CAS rồi
//! materialize layout tương thích ecosystem bên dưới các gốc này.

use mgc_types::{MgError, MgResult};
use std::path::PathBuf;

/// A MagiCore-owned install root for one ecosystem.
/// Gốc cài do MagiCore sở hữu cho một ecosystem.
pub struct SharedStoreRun {
    /// Native package layout root inside the MagiCore store.
    /// Gốc layout package native bên trong store MagiCore.
    pub install_root: PathBuf,
}

/// Resolve and create a MagiCore-owned install root for an ecosystem.
/// Resolve và tạo gốc cài do MagiCore quản lý cho một ecosystem.
fn shared_root(eco: &str) -> MgResult<PathBuf> {
    let globals = mgc_platform::paths::GlobalPaths::new()
        .map_err(|e| MgError::Other(format!("cannot resolve mgc home: {e}")))?;
    let root = globals.store.join(eco);
    std::fs::create_dir_all(&root).map_err(|e| {
        MgError::Other(format!(
            "cannot create MagiCore install root '{}': {e}",
            root.display()
        ))
    })?;
    Ok(root)
}

impl SharedStoreRun {
    /// Return the MagiCore-owned Cargo-layout materialization root.
    /// Trả gốc materialize theo layout Cargo do MagiCore quản lý.
    pub fn cargo() -> MgResult<Self> {
        Ok(Self {
            install_root: shared_root("cargo")?,
        })
    }

    /// Return the MagiCore-owned Python artifact/materialization root.
    /// Trả gốc artifact/materialize Python do MagiCore quản lý.
    pub fn pypi() -> MgResult<Self> {
        Ok(Self {
            install_root: shared_root("pypi")?,
        })
    }

    /// Return the MagiCore-owned Go module materialization root.
    /// Trả gốc materialize module Go do MagiCore quản lý.
    pub fn go() -> MgResult<Self> {
        Ok(Self {
            install_root: shared_root("go")?,
        })
    }

    /// Return the MagiCore-owned Maven artifact materialization root.
    /// Trả gốc materialize artifact Maven do MagiCore quản lý.
    pub fn maven() -> MgResult<Self> {
        Ok(Self {
            install_root: shared_root("maven")?,
        })
    }

    /// Return the MagiCore-owned NuGet package materialization root.
    /// Trả gốc materialize package NuGet do MagiCore quản lý.
    pub fn nuget() -> MgResult<Self> {
        Ok(Self {
            install_root: shared_root("nuget")?,
        })
    }
}
