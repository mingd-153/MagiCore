mod content;

use std::path::{Path, PathBuf};

use crate::engine::{ProjectCreated, ScaffoldContext, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::templates::{write_favicon, Template};
use crate::validate::NameValidator;

pub use content::*;

pub struct AstroCodegen;

impl AstroCodegen {
    fn generate_files(name: &str, version: &str) -> Vec<(String, String)> {
        let ctx = Ctx::new(name, version);
        vec![
            ("package.json".into(), package_json(&ctx)),
            ("tsconfig.json".into(), tsconfig_json()),
            ("astro.config.mjs".into(), astro_config_mjs()),
            ("src/env.d.ts".into(), env_dts()),
            ("src/pages/index.astro".into(), index_astro(&ctx)),
            ("src/pages/about.astro".into(), about_astro(&ctx)),
            ("src/layouts/Layout.astro".into(), layout_astro()),
            ("src/components/Header.astro".into(), header_astro()),
            ("src/styles/global.css".into(), global_css()),
            (".gitignore".into(), gitignore()),
            (".env.example".into(), env_example()),
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

impl ScaffoldEngine for AstroCodegen {
    fn name(&self) -> &str { "astro" }

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
        name: "astro",
        description: "Astro static site with TypeScript",
        commands: &["astro"],
        supported_flags: &[],
        create_engine: || -> Box<dyn ScaffoldEngine> { Box::new(AstroCodegen) },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::versions::*;

    #[test]
    fn test_astro_generate_content() {
        let ctx = Ctx::new("my-astro-site", "1.0.0");
        let pkg = package_json(&ctx);
        assert!(pkg.contains("astro"));
        assert!(pkg.contains("my-astro-site"));
        assert!(pkg.contains(ASTRO()));

        let idx = index_astro(&ctx);
        assert!(idx.contains("my-astro-site"));

        let readme_content = readme(&ctx);
        assert!(readme_content.contains("MyAstroSite"));
    }

    #[test]
    fn test_astro_codegen_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        let engine = AstroCodegen;
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-astro-site".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-astro-site", dest.clone()).with_vars(vars);
        let result = engine.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 12);
        assert!(dest.join("package.json").exists());
        assert!(dest.join("src/pages/index.astro").exists());
        assert!(dest.join("astro.config.mjs").exists());
    }

    #[test]
    fn test_astro_readme_pascal_case() {
        let ctx = Ctx::new("my-site", "1.0.0");
        let readme_content = readme(&ctx);
        assert!(readme_content.contains("MySite"));
    }

    #[test]
    fn test_astro_engine_name() {
        let engine = AstroCodegen;
        assert_eq!(engine.name(), "astro");
    }

    #[test]
    fn test_astro_template() {
        let tpl = template();
        assert_eq!(tpl.name, "astro");
        assert_eq!(tpl.commands, &["astro"]);
        let engine = (tpl.create_engine)();
        assert_eq!(engine.name(), "astro");
    }

    #[test]
    fn test_astro_files_content() {
        let files = AstroCodegen::generate_files("test-site", "2.0.0");
        let file_map: std::collections::HashMap<&str, &str> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        assert!(file_map.contains_key("package.json"));
        assert!(file_map.contains_key("src/pages/index.astro"));
        assert!(file_map.contains_key("src/layouts/Layout.astro"));
        assert!(file_map.contains_key("src/components/Header.astro"));

        let pkg = file_map.get("package.json").unwrap();
        assert!(pkg.contains("astro"));
        assert!(pkg.contains("test-site"));
        assert!(pkg.contains("2.0.0"));

        let idx = file_map.get("src/pages/index.astro").unwrap();
        assert!(idx.contains("test-site"));

        let about = file_map.get("src/pages/about.astro").unwrap();
        assert!(about.contains("test-site"));
    }
}
