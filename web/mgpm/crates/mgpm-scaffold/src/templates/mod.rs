pub mod vanilla;
pub mod react;
pub mod next;
pub mod vue;
pub mod astro;
pub mod sveltekit;
pub mod nuxt;
pub mod tanstack;
pub mod react_router;
pub mod nestjs;
pub mod hono;
pub mod express;
pub mod fastify;
pub mod fastapi;
pub mod go_gin;
pub mod trpc;
pub mod spring_boot;
pub mod laravel;
pub mod koa;
pub mod django;
pub mod flask;
pub mod rails;
pub mod phoenix;
pub mod axum;
pub mod actix;
pub mod echo;
pub mod fiber;

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
        self.register(astro::template());
        self.register(sveltekit::template());
        self.register(nuxt::template());
        self.register(tanstack::template());
        self.register(react_router::template());
        self.register(express::template());
        self.register(fastify::template());
        self.register(fastapi::template());
        self.register(go_gin::template());
        self.register(nestjs::template());
        self.register(hono::template());
        self.register(trpc::template());
        self.register(spring_boot::template());
        self.register(django::template());
        self.register(flask::template());
        self.register(rails::template());
        self.register(phoenix::template());
        self.register(laravel::template());
        self.register(koa::template());
        self.register(axum::template());
        self.register(actix::template());
        self.register(echo::template());
        self.register(fiber::template());
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
    if !path.starts_with(base) {
        return Err(ScaffoldError::IoError {
            context: format!("path traversal detected: {relative}"),
            source: std::io::Error::other("path escapes base directory"),
        });
    }
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
