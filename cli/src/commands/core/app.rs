use anyhow::Result;

const CORE_NAME: &str = "app";

fn not_available() -> anyhow::Error {
    anyhow::anyhow!(
        "'{CORE_NAME}' core is under development. Only the 'web' core is available in this release."
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    _version: Option<String>,
    _dev: bool,
    _exact: bool,
    _optional: bool,
    _peer: bool,
    _no_save: bool,
    _global: bool,
) -> Result<()> {
    let _ = packages;
    Err(not_available())
}
pub async fn remove(_packages: Vec<String>) -> Result<()> {
    Err(not_available())
}
pub async fn list() -> Result<()> {
    Err(not_available())
}
pub async fn update(_packages: Vec<String>, _install: bool) -> Result<()> {
    Err(not_available())
}
pub async fn install(_packages: Vec<String>) -> Result<()> {
    Err(not_available())
}
pub mod create {
    pub async fn run(_framework: &str, _project_name: &str) -> anyhow::Result<()> {
        Err(super::not_available())
    }
}
