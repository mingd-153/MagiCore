//! `mgc remove` (auto-detect core) — programmatic entry. Delegates to the
//! detected core's OWN remove lane (the same function the CLI runs) so the
//! C0 ownership firewall gates uniformly — a direct adapter call here
//! would spawn toolchains with no gate, no compat opt-in, no audit.
//! (Lối remove dùng chung: gọi đúng lane của core để gate C0.)

use crate::context::ProjectContext;
use anyhow::Result;

/// mgc remove — remove a dependency from the project
#[allow(dead_code)]
pub async fn run(package: String, core: Option<&str>) -> Result<()> {
    run_many(vec![package], core, None).await
}

#[allow(dead_code)]
pub async fn run_many(
    packages: Vec<String>,
    core: Option<&str>,
    compat_runtime: Option<String>,
) -> Result<()> {
    // Fail closed outside a project AND outside the firewall: the detected
    // core selects its own lane (each gates before any spawn).
    // (Chặn ngoài project và ngoài tường lửa: lane của core sẽ gate.)
    let ctx = ProjectContext::load_with_core(core)?;
    // Post-remove reinstall follows each lane's CLI default.
    // (Sau remove có reinstall theo default của lane CLI.)
    const MCP_INSTALL_AFTER_REMOVE: bool = false;
    match ctx.config.ecosystem.as_str() {
        #[cfg(feature = "web")]
        "web" => {
            super::core::remove::web::remove(packages, MCP_INSTALL_AFTER_REMOVE, compat_runtime)
                .await
        }
        #[cfg(feature = "game")]
        "game" => super::core::remove::game::remove(packages, compat_runtime).await,
        #[cfg(feature = "ai")]
        "ai" => super::core::remove::ai::remove(packages, compat_runtime).await,
        #[cfg(feature = "clo")]
        "clo" => super::core::remove::clo::remove(packages, compat_runtime).await,
        #[cfg(feature = "cicd")]
        "cicd" => super::core::remove::cicd::remove(packages, compat_runtime).await,
        #[cfg(feature = "iot")]
        "iot" => super::core::remove::iot::remove(packages, compat_runtime).await,
        #[cfg(feature = "app")]
        "app" => super::core::remove::app::remove(packages, compat_runtime).await,
        #[cfg(feature = "lib")]
        "lib" => super::core::remove::library::remove(packages, compat_runtime).await,
        other => Err(if is_known_core(other) {
            crate::error::core_not_in_build(other)
        } else {
            crate::error::unknown_core(other)
        }),
    }
}

/// Core names the firewall knows (mirrors the dispatch core set).
/// (Tên core tường lửa biết.)
fn is_known_core(name: &str) -> bool {
    matches!(
        name,
        "web" | "game" | "ai" | "clo" | "cicd" | "iot" | "app" | "lib" | "hardware"
    )
}
