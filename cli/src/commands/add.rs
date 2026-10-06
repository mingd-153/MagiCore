//! `mgc add` (auto-detect core) — programmatic entry shared by the MCP
//! `mgc_add` tool. Delegates to the detected core's OWN add lane (the same
//! function the CLI runs) so the C0 ownership firewall gates uniformly —
//! a direct adapter call here would spawn toolchains with no gate, no
//! compat opt-in, no audit.
//! (Lối add dùng chung cho MCP: gọi đúng lane của core để gate C0.)

use crate::context::ProjectContext;
use anyhow::Result;

/// mgc add — add a dependency to the project
#[allow(dead_code)]
// Mirrors the public CLI flag surface — giữ nguyên ánh xạ trực tiếp với các cờ CLI công khai.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    package: String,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
    core: Option<&str>,
) -> Result<()> {
    run_many(
        vec![package],
        version,
        dev,
        exact,
        optional,
        peer,
        no_save,
        global,
        core,
        None,
    )
    .await
}

// Mirrors the public CLI flag surface — giữ nguyên ánh xạ trực tiếp với các cờ CLI công khai.
#[allow(clippy::too_many_arguments)]
pub async fn run_many(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
    core: Option<&str>,
    compat_runtime: Option<String>,
) -> Result<()> {
    // Fail closed outside a project AND outside the firewall: the detected
    // core selects its own lane (each gates before any spawn).
    // `compat_runtime` is None for MCP today (Native mode), so delegated
    // lanes fail closed naming the required flag.
    // (Chặn ngoài project và ngoài tường lửa: lane của core sẽ gate.)
    let ctx = ProjectContext::load_with_core(core)?;
    // Post-add install follows each lane's CLI default (web honors the
    // flag below; lib/game/ai/clo/iot/app lanes install like their CLI
    // counterparts). MCP previously never installed — unification with
    // the CLI is intentional: one behavior, one gate, no shadow path.
    // (Sau add có install theo default của lane CLI — thống nhất.)
    const MCP_INSTALL_AFTER_ADD: bool = false;
    match ctx.config.ecosystem.as_str() {
        #[cfg(feature = "web")]
        "web" => {
            super::core::add::web::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                MCP_INSTALL_AFTER_ADD,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "game")]
        "game" => {
            super::core::add::game::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "ai")]
        "ai" => {
            super::core::add::ai::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "clo")]
        "clo" => {
            super::core::add::clo::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "cicd")]
        "cicd" => {
            super::core::add::cicd::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "iot")]
        "iot" => {
            super::core::add::iot::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "app")]
        "app" => {
            super::core::add::app::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "lib")]
        "lib" => {
            super::core::add::library::add(
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                compat_runtime,
            )
            .await
        }
        #[cfg(feature = "hardware")]
        "hardware" => super::core::add::hardware::add(packages, compat_runtime, version).await,
        other => Err(if is_known_core(other) {
            // Known core whose lane is compiled out of THIS build —
            // fail closed, never fall through to a direct adapter call.
            // (Core quen nhưng lane không có trong build này.)
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
