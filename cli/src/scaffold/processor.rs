use anyhow::Result;
use std::path::Path;
use crate::wizard::engine::ScaffoldConfig;

pub struct Scaffolder;

impl Scaffolder {
    pub fn scaffold(config: &ScaffoldConfig) -> Result<()> {
        let target = Path::new(&config.project_name);
        if target.exists() {
            anyhow::bail!("Directory '{}' already exists", config.project_name);
        }

        std::fs::create_dir_all(target)?;

        mg_ui::success(&format!("Created {} project: {}", config.core, config.project_name));
        Ok(())
    }
}
