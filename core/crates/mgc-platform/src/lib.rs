//! mgc-platform — L0 OS layer (MagiCore)
//! OS layer, symlink handling, shell abstraction, standard paths, permissions.
//! OS layer for paths, symlinks, shells, and permissions — lớp OS quản lý path, symlink, shell và quyền.
//!
//! Modules: paths (standard paths), os, symlink, shell, perms, reflink.

pub mod fs_semaphore;
pub mod paths;
pub mod reflink;
pub use fs_semaphore::{MAX_CONCURRENT_FS_WRITES, global_fs_write_semaphore};
pub mod prelude {}
