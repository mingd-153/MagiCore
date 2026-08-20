//! `mg remove` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod iot;
pub mod library;
pub mod web;

pub async fn run(core: &str, packages: Vec<String>, install: bool) -> Result<()> {
    match core {
        "game" => game::remove(packages).await,
        "ai" => ai::remove(packages).await,
        "web" => web::remove(packages, install).await,
        "clo" => clo::remove(packages).await,
        "cicd" => cicd::remove(packages).await,
        "iot" => iot::remove(packages).await,
        "app" => app::remove(packages).await,
        "lib" => library::remove(packages).await,
        other => Err(crate::error::unknown_core(other)),
    }
}
