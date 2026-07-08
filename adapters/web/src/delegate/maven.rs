/// Java/Maven delegate adapter
use anyhow::Result;

pub struct MavenDelegate;

impl MavenDelegate {
    pub fn install(_project_dir: &std::path::Path) -> Result<()> {
        Ok(())
    }
}
