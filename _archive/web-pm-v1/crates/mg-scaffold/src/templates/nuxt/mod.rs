mod content;

use std::path::{Path, PathBuf};

use crate::engine::{ProjectCreated, ScaffoldContext, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::templates::{write_favicon, Template};
use crate::validate::NameValidator;

pub use content::*;

pub struct NuxtCodegen;

impl NuxtCodegen {
    fn generate_files(name: &str, version: &str) -> Vec<(String, String)> {
        let ctx = Ctx::new(name, version);
        vec![
            ("package.json".into(), package_json(&ctx)),
            ("nuxt.config.ts".into(), nuxt_config_ts().into()),
            ("tsconfig.json".into(), tsconfig_json().into()),
            ("app.vue".into(), app_vue().into()),
            ("pages/index.vue".into(), index_vue(&ctx)),
            ("pages/about.vue".into(), about_vue(&ctx)),
            ("components/Header.vue".into(), header_vue().into()),
            ("composables/useAuth.ts".into(), use_auth_ts().into()),
            ("stores/auth.ts".into(), auth_store_ts().into()),
            ("services/api.ts".into(), api_ts().into()),
            ("types/index.ts".into(), types_index_ts().into()),
            ("utils/helpers.ts".into(), helpers_ts().into()),
            ("assets/css/main.css".into(), main_css().into()),
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

impl ScaffoldEngine for NuxtCodegen {
    fn name(&self) -> &str { "nuxt" }

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
        name: "nuxt",
        description: "Nuxt 3 Vue web app with TypeScript",
        commands: &["nuxt"],
        supported_flags: &[],
        create_engine: || -> Box<dyn ScaffoldEngine> { Box::new(NuxtCodegen) },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::versions::*;

    #[test]
    fn test_nuxt_generate_content() {
        let ctx = Ctx::new("my-nuxt-app", "1.0.0");
        let pkg = package_json(&ctx);
        assert!(pkg.contains("nuxt"));
        assert!(pkg.contains("my-nuxt-app"));
        assert!(pkg.contains(NUXT()));

        let idx = index_vue(&ctx);
        assert!(idx.contains("my-nuxt-app"));

        let readme_content = readme(&ctx);
        assert!(readme_content.contains("my-nuxt-app"));
    }

    #[test]
    fn test_nuxt_codegen_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        let engine = NuxtCodegen;
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-nuxt-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-nuxt-app", dest.clone()).with_vars(vars);
        let result = engine.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 15);
        assert!(dest.join("package.json").exists());
        assert!(dest.join("app.vue").exists());
        assert!(dest.join("pages/index.vue").exists());
        assert!(dest.join("nuxt.config.ts").exists());
    }

    #[test]
    fn test_nuxt_engine_name() {
        let engine = NuxtCodegen;
        assert_eq!(engine.name(), "nuxt");
    }

    #[test]
    fn test_nuxt_template() {
        let tpl = template();
        assert_eq!(tpl.name, "nuxt");
        assert_eq!(tpl.commands, &["nuxt"]);
        let engine = (tpl.create_engine)();
        assert_eq!(engine.name(), "nuxt");
    }

    #[test]
    fn test_nuxt_files_content() {
        let files = NuxtCodegen::generate_files("test-nuxt", "2.0.0");
        let file_map: std::collections::HashMap<&str, &str> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        assert!(file_map.contains_key("package.json"));
        assert!(file_map.contains_key("app.vue"));
        assert!(file_map.contains_key("pages/index.vue"));
        assert!(file_map.contains_key("components/Header.vue"));

        let pkg = file_map.get("package.json").unwrap();
        assert!(pkg.contains("nuxt"));
        assert!(pkg.contains("test-nuxt"));
        assert!(pkg.contains("2.0.0"));

        let idx = file_map.get("pages/index.vue").unwrap();
        assert!(idx.contains("test-nuxt"));

        let about = file_map.get("pages/about.vue").unwrap();
        assert!(about.contains("test-nuxt"));
    }
}
