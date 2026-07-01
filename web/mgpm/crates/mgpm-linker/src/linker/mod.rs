use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use mgpm_store::store::cas::ContentStore;
use mgpm_store::store::gvs::GlobalVirtualStore;

pub type RefcountCallback = Arc<dyn Fn(&str) -> io::Result<()> + Send + Sync>;

/// Strategy for linking node_modules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkerStrategy {
    /// npm-style flat node_modules (default)
    Hoisted,
    /// pnpm-style strict, symlinked node_modules
    Isolated,
    /// Yarn PnP-style (future — skeleton only)
    Pnp,
}

impl LinkerStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hoisted => "hoisted",
            Self::Isolated => "isolated",
            Self::Pnp => "pnp",
        }
    }
}

impl std::str::FromStr for LinkerStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hoisted" => Ok(Self::Hoisted),
            "isolated" => Ok(Self::Isolated),
            "pnp" => Ok(Self::Pnp),
            _ => Err(format!(
                "unknown linker strategy: '{}'. Use 'hoisted', 'isolated', or 'pnp'",
                s
            )),
        }
    }
}

impl std::fmt::Display for LinkerStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Linker trait — defines the interface for all linker strategies
pub trait Linker: Send + Sync {
    /// Link all packages into node_modules
    fn link_all(
        &self,
        packages: &[PackageLinkInfo],
        store: &ContentStore,
        project_root: &Path,
    ) -> Result<LinkResult, LinkError>;

    /// Link a single package
    fn link_package(
        &self,
        pkg: &PackageLinkInfo,
        store: &ContentStore,
        dest: &Path,
    ) -> Result<(), LinkError>;

    /// Create binary symlinks
    fn link_bins(
        &self,
        packages: &[PackageLinkInfo],
        store: &ContentStore,
        bin_dir: &Path,
    ) -> Result<(), LinkError>;

    /// Unlink a package from node_modules
    fn unlink_package(&self, name: &str, project_root: &Path) -> Result<(), LinkError>;

    /// Get the linker strategy
    fn strategy(&self) -> LinkerStrategy;
}

#[derive(Clone)]
pub struct LinkerOptions {
    pub project_root: PathBuf,
    pub virtual_store_dir: PathBuf,
    pub global_virtual_store: bool,
    pub hoist: bool,
    pub hoist_pattern: Vec<String>,
    pub symlinks: bool,
    pub store_path: PathBuf,
    pub refcount_callback: Option<RefcountCallback>,
    pub workspace: Option<mgpm_workspace::Workspace>,
    pub strategy: LinkerStrategy,
    pub gvs_root: PathBuf,
}

impl std::fmt::Debug for LinkerOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkerOptions")
            .field("project_root", &self.project_root)
            .field("virtual_store_dir", &self.virtual_store_dir)
            .field("global_virtual_store", &self.global_virtual_store)
            .field("hoist", &self.hoist)
            .field("hoist_pattern", &self.hoist_pattern)
            .field("symlinks", &self.symlinks)
            .field("store_path", &self.store_path)
            .field("refcount_callback", &self.refcount_callback.as_ref().map(|_| "Box<dyn Fn>"))
            .field("workspace", &self.workspace)
            .field("strategy", &self.strategy)
            .field("gvs_root", &self.gvs_root)
            .finish()
    }
}

impl Default for LinkerOptions {
    fn default() -> Self {
        Self {
            project_root: PathBuf::from("."),
            virtual_store_dir: PathBuf::from(".mgpm"),
            global_virtual_store: false,
            hoist: false,
            hoist_pattern: vec!["*".to_string()],
            symlinks: true,
            store_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mgpm")
                .join("store"),
            refcount_callback: None,
            workspace: None,
            strategy: LinkerStrategy::Hoisted,
            gvs_root: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mgpm")
                .join("gvs")
                .join("v1"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageLinkInfo {
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
    pub peer_dependencies: Vec<(String, String)>,
    pub files: Vec<(String, String)>,
    pub is_root_dep: bool,
    pub bin_entries: Vec<(String, String)>,
    pub total_size: u64,
    pub dep_graph_hash: String,
}

#[allow(clippy::too_many_arguments)]
impl PackageLinkInfo {
    pub fn new(
        name: String,
        version: String,
        dependencies: Vec<String>,
        peer_dependencies: Vec<(String, String)>,
        files: Vec<(String, String)>,
        is_root_dep: bool,
        bin_entries: Vec<(String, String)>,
        total_size: u64,
        dep_graph_hash: String,
    ) -> Self {
        Self {
            name,
            version,
            dependencies,
            peer_dependencies,
            files,
            is_root_dep,
            bin_entries,
            total_size,
            dep_graph_hash,
        }
    }
}

pub struct LinkerFactory;

impl LinkerFactory {
    pub fn create(
        options: LinkerOptions,
        _store: &mgpm_store::store::cas::ContentStore,
    ) -> Result<Box<dyn Linker>, LinkError> {
        match options.strategy {
            LinkerStrategy::Hoisted => Ok(Box::new(HoistedLinker::new(options))),
            LinkerStrategy::Isolated => {
                let gvs = GlobalVirtualStore::new(options.gvs_root.clone());
                Ok(Box::new(IsolatedLinker::new(gvs, options)))
            }
            LinkerStrategy::Pnp => Err(LinkError::Io(io::Error::new(
                io::ErrorKind::Unsupported,
                "PnP linker not yet implemented",
            ))),
        }
    }
}

mod hoisted;
mod isolated;

pub use hoisted::HoistedLinker;
pub use isolated::IsolatedLinker;

#[derive(Debug, Clone)]
pub struct LinkResult {
    pub linked: Vec<PackageLinkResult>,
    pub node_modules_path: PathBuf,
    pub dep_graph_hash: String,
}

#[derive(Debug, Clone)]
pub struct PackageLinkResult {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub peer_hash: String,
    pub linked_deps: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("linker error: {0}")]
    Other(String),
}

fn create_relative_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        return Ok(());
    }

    // Validate destination doesn't escape intended directory
    if dst.components().any(|c| c == std::path::Component::ParentDir) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("destination path contains '..': {}", dst.display()),
        ));
    }

    // Create parent directory atomically before symlink
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    let relative = make_relative(dst, src)?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&relative, dst)?;
    }
    #[cfg(not(unix))]
    {
        if relative.is_dir() {
            std::os::windows::fs::symlink_dir(&relative, dst)?;
        } else {
            std::os::windows::fs::symlink_file(&relative, dst)?;
        }
    }
    Ok(())
}

/// Validate that a relative path does not contain parent directory components
fn validate_rel_path(path: &str) -> Result<(), LinkError> {
    if path.contains("..") {
        return Err(LinkError::Other(format!(
            "path contains parent directory traversal: '{}'",
            path
        )));
    }
    Ok(())
}

fn make_relative(base: &Path, target: &Path) -> io::Result<PathBuf> {
    let abs_base = if base.is_absolute() {
        base.to_path_buf()
    } else {
        std::env::current_dir()?.join(base)
    };
    let abs_target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()?.join(target)
    };

    let base_components: Vec<_> = abs_base.components().collect();
    let target_components: Vec<_> = abs_target.components().collect();

    let common_len = base_components
        .iter()
        .zip(target_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = PathBuf::new();

    if common_len >= base_components.len() {
        for component in &target_components[common_len..] {
            result.push(component);
        }
        return Ok(result);
    }

    for _ in common_len..base_components.len().saturating_sub(1) {
        result.push("..");
    }

    for component in &target_components[common_len..] {
        result.push(component);
    }

    Ok(result)
}