mod content;

use std::path::{Path, PathBuf};

use crate::engine::{ProjectCreated, ScaffoldContext, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::templates::{write_favicon, Template};
use crate::validate::NameValidator;

pub use content::*;

pub struct FastifyCodegen;

impl FastifyCodegen {
    fn generate_files(name: &str, version: &str) -> Vec<(String, String)> {
        let ctx = Ctx::new(name, version);
        vec![
            ("package.json".into(), package_json(&ctx)),
            ("tsconfig.json".into(), tsconfig_json().into()),
            ("src/index.ts".into(), index_ts().into()),
            ("src/app.ts".into(), app_ts().into()),
            ("src/routes/items.ts".into(), items_ts().into()),
            ("src/types/index.ts".into(), types_index_ts().into()),
            ("Dockerfile".into(), dockerfile().into()),
            (".gitignore".into(), gitignore().into()),
            (".env.example".into(), env_example().into()),
            ("README.md".into(), readme(&ctx)),
        ]
    }

    fn resolve_dest(ctx: &ScaffoldContext) -> Result<PathBuf, ScaffoldError> {
        let base = std::env::current_dir().map_err(|e| ScaffoldError::IoError {
            context: "current_dir".to_string(), source: e,
        })?;
        Ok(if ctx.project_path.is_absolute() { ctx.project_path.clone() }
           else { base.join(&ctx.project_path) })
    }

    fn write_files(dest: &Path, files: Vec<(String, String)>, force: bool) -> Result<Vec<PathBuf>, ScaffoldError> {
        let mut created = Vec::new();
        for (rel_path, content) in files {
            let dest_path = dest.join(&rel_path);
            if dest_path.exists() {
                if !force { return Err(ScaffoldError::PathExists(dest_path)); }
                if dest_path.is_file() { std::fs::remove_file(&dest_path)?; }
            }
            if let Some(parent) = dest_path.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::write(&dest_path, content)?;
            created.push(dest_path);
        }
        Ok(created)
    }
}

impl ScaffoldEngine for FastifyCodegen {
    fn name(&self) -> &str { "fastify" }

    fn create_project(&self, ctx: &ScaffoldContext, force: bool) -> Result<ProjectCreated, ScaffoldError> {
        NameValidator::validate(&ctx.project_name).map_err(|e| {
            ScaffoldError::InvalidName(ctx.project_name.clone(), e.to_string())
        })?;
        let name = &ctx.project_name;
        let version = ctx.get_var("version").unwrap_or("1.0.0");
        let dest = Self::resolve_dest(ctx)?;
        let files = Self::generate_files(name, version);
        let mut created = Self::write_files(&dest, files, force)?;
        write_favicon(&dest)?;
        created.push(dest.join("public").join("favicon.ico"));
        Ok(ProjectCreated {
            name: name.clone(),
            path: dest,
            files_created: created,
            features: ctx.features.clone(),
        })
    }
}

pub fn template() -> Template {
    Template {
        name: "fastify",
        description: "Fastify REST API with TypeScript",
        commands: &["fastify"],
        supported_flags: &[],
        create_engine: || -> Box<dyn ScaffoldEngine> { Box::new(FastifyCodegen) },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::versions::*;

    #[test]
    fn test_fastify_generate_content() {
        let ctx = Ctx::new("my-fastify-api", "1.0.0");
        let pkg = package_json(&ctx);
        assert!(pkg.contains("fastify"));
        assert!(pkg.contains("my-fastify-api"));
        assert!(pkg.contains(FASTIFY()));

        let readme_content = readme(&ctx);
        assert!(readme_content.contains("my-fastify-api"));
    }

    #[test]
    fn test_fastify_codegen_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        let engine = FastifyCodegen;
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-fastify-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-fastify-api", dest.clone()).with_vars(vars);
        let result = engine.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 11);
        assert!(dest.join("package.json").exists());
        assert!(dest.join("src/app.ts").exists());
        assert!(dest.join("src/routes/items.ts").exists());
    }

    #[test]
    fn test_fastify_engine_name() {
        let engine = FastifyCodegen;
        assert_eq!(engine.name(), "fastify");
    }

    #[test]
    fn test_fastify_template() {
        let tpl = template();
        assert_eq!(tpl.name, "fastify");
        assert_eq!(tpl.commands, &["fastify"]);
        let engine = (tpl.create_engine)();
        assert_eq!(engine.name(), "fastify");
    }

    #[test]
    fn test_fastify_files_content() {
        let files = FastifyCodegen::generate_files("test-api", "2.0.0");
        let file_map: std::collections::HashMap<&str, &str> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        assert!(file_map.contains_key("package.json"));
        assert!(file_map.contains_key("src/index.ts"));
        assert!(file_map.contains_key("src/app.ts"));
        assert!(file_map.contains_key("src/routes/items.ts"));

        let pkg = file_map.get("package.json").unwrap();
        assert!(pkg.contains("fastify"));
        assert!(pkg.contains("test-api"));
        assert!(pkg.contains("2.0.0"));

        let readme_content = file_map.get("README.md").unwrap();
        assert!(readme_content.contains("test-api"));
    }
}
