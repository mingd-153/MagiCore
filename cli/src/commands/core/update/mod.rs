//! `mg update` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod web;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod iot;
pub mod library;

pub async fn run(core: &str, packages: Vec<String>, install: bool) -> Result<()> {
    match core {
        "game" => game::update(packages, install).await,
        "ai" => ai::update(packages, install).await,
        "web" => web::update(packages, install).await,
                "clo" => clo::update(packages, install).await,
        "cicd" => cicd::update(packages, install).await,
        "iot" => iot::update(packages, install).await,
        "app" => app::update(packages, install).await,
        "lib" => library::update(packages, install).await,
        other => Err(crate::error::unknown_core(other)),
    }
}