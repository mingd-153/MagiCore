//! `mgc update` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

#[cfg(feature = "ai")]
pub mod ai;
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "cicd")]
pub mod cicd;
#[cfg(feature = "clo")]
pub mod clo;
#[cfg(feature = "game")]
pub mod game;
#[cfg(feature = "iot")]
pub mod iot;
#[cfg(feature = "lib")]
pub mod library;
pub mod web;

pub async fn run(core: &str, packages: Vec<String>, install: bool) -> Result<()> {
    match core {
        #[cfg(feature = "game")]
        "game" => game::update(packages, install).await,
        #[cfg(not(feature = "game"))]
        "game" => Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        "ai" => ai::update(packages, install).await,
        #[cfg(not(feature = "ai"))]
        "ai" => Err(crate::error::core_not_in_build("ai")),
        "web" => web::update(packages, install).await,
        #[cfg(feature = "clo")]
        "clo" => clo::update(packages, install).await,
        #[cfg(not(feature = "clo"))]
        "clo" => Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        "cicd" => cicd::update(packages, install).await,
        #[cfg(not(feature = "cicd"))]
        "cicd" => Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        "iot" => iot::update(packages, install).await,
        #[cfg(not(feature = "iot"))]
        "iot" => Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        "app" => app::update(packages, install).await,
        #[cfg(not(feature = "app"))]
        "app" => Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "lib")]
        "lib" => library::update(packages, install).await,
        #[cfg(not(feature = "lib"))]
        "lib" => Err(crate::error::core_not_in_build("lib")),
        other => Err(crate::error::unknown_core(other)),
    }
}
