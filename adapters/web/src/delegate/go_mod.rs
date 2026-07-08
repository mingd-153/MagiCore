/// Go module delegate adapter
use anyhow::Result;

pub struct GoModDelegate;

impl GoModDelegate {
    pub fn install(_project_dir: &std::path::Path) -> Result<()> {
        Ok(())
    }
}
