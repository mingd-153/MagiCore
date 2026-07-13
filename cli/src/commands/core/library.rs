use anyhow::Result;

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
    anyhow::bail!("Library adapter is not yet implemented")
}
pub async fn remove(_package: String) -> Result<()> {
    anyhow::bail!("Library adapter is not yet implemented")
}
pub async fn list() -> Result<()> {
    anyhow::bail!("Library adapter is not yet implemented")
}
pub async fn update(_packages: Vec<String>) -> Result<()> {
    anyhow::bail!("Library adapter is not yet implemented")
}
pub async fn install(_packages: Vec<String>) -> Result<()> {
    anyhow::bail!("Library adapter is not yet implemented")
}
pub mod create {
    pub async fn run(_project_name: &str) -> anyhow::Result<()> {
        anyhow::bail!("Library adapter is not yet implemented")
    }
}
