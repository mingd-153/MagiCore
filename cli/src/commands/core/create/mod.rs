//! `mg create-<core>` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

use anyhow::Result;

pub mod ai;
pub mod app;
pub mod web;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod iot;
pub mod library;
pub mod hardware;

pub async fn run(core: &str, framework: &str, project_name: &str) -> Result<()> {
    match core {
        "game" => game::run(framework, project_name).await,
        "ai" => ai::run(framework, project_name).await,
        "clo" => clo::run(framework, project_name).await,
        other => Err(crate::error::unknown_core(other)),
    }
}
