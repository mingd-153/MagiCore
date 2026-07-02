pub mod vanilla;
pub mod react;
pub mod next;
pub mod vue;

use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    pub commands: &'static [&'static str],
    pub create_engine: fn() -> Box<dyn ScaffoldEngine>,
}

pub struct TemplateRegistry {
    templates: Vec<Template>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self { templates: vec![] }
    }

    pub fn register(&mut self, template: Template) {
        self.templates.push(template);
    }

    pub fn register_defaults(&mut self) {
        self.register(vanilla::template());
        self.register(react::template());
        self.register(next::template());
        self.register(vue::template());
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.iter().find(|t| t.name == name)
    }

    pub fn find_by_command(&self, command: &str) -> Option<&Template> {
        self.templates
            .iter()
            .find(|t| t.commands.contains(&command))
    }

    pub fn list(&self) -> &[Template] {
        &self.templates
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn write_template(base: &Path, relative: &str, content: &str) -> Result<PathBuf, ScaffoldError> {
    let path = base.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScaffoldError::IoError {
            context: format!("create template dir {}", parent.display()),
            source: e,
        })?;
    }
    std::fs::write(&path, content).map_err(|e| ScaffoldError::IoError {
        context: format!("write template {}", path.display()),
        source: e,
    })?;
    Ok(path)
}

pub fn extract_embedded(base: &Path, files: &HashMap<&str, &str>) -> Result<(), ScaffoldError> {
    for (relative, content) in files {
        write_template(base, relative, content)?;
    }
    Ok(())
}
