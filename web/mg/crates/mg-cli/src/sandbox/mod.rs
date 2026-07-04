//! Sandbox module for secure package installation
//!
//! macOS: Seatbelt sandbox profile via sandbox-init
//! Linux: Landlock LSM (kernel 5.13+)

use std::path::Path;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

/// Enable sandbox for the given operation
pub fn enable_sandbox(project_dir: &Path) -> Result<SandboxGuard, String> {
    #[cfg(target_os = "macos")]
    return macos::apply_seatbelt(project_dir);

    #[cfg(target_os = "linux")]
    return linux::apply_landlock(project_dir);

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        eprintln!("Warning: Sandbox not supported on this platform");
        Ok(SandboxGuard {})
    }
}

pub struct SandboxGuard {}

impl Drop for SandboxGuard {
    fn drop(&mut self) {
        // Sandbox is automatically lifted when process exits on macOS
        // Landlock is permanent for the process lifetime
        eprintln!("Sandbox lifted");
    }
}
