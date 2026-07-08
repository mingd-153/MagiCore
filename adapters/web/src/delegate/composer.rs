/// PHP/Composer delegate adapter
use anyhow::Result;

pub struct ComposerDelegate;

impl ComposerDelegate {
    pub fn install(_project_dir: &std::path::Path) -> Result<()> {
        Ok(())
    }
}
