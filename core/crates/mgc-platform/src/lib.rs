//! mgc-platform — L0 OS layer (MagiCore)
//! OS layer, symlink handling, shell abstraction, standard paths, permissions.
//! (Lớp OS: os, symlink, shell, path chuẩn, quyền — chi tiết sys-mgc/11 §6, sys-mgc/14)
//!
//! Modules: paths (standard paths), os, symlink, shell, perms, reflink.

pub mod fs_semaphore;
pub mod paths;
pub mod reflink;
pub use fs_semaphore::{MAX_CONCURRENT_FS_WRITES, global_fs_write_semaphore};
pub mod prelude {}
