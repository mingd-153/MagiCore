/// Vite build pipeline
use anyhow::Result;

pub struct ViteBuilder;

impl ViteBuilder {
    pub fn build(_project_dir: &std::path::Path) -> Result<()> {
        Ok(())
    }
}
