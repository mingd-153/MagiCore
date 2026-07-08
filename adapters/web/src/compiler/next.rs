/// Next.js build pipeline
use anyhow::Result;

pub struct NextBuilder;

impl NextBuilder {
    pub fn build(_project_dir: &std::path::Path) -> Result<()> {
        Ok(())
    }
}
