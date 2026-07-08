mod content;

use std::path::{Path, PathBuf};

use crate::engine::{ProjectCreated, ScaffoldContext, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::templates::{write_favicon, Template};
use crate::validate::NameValidator;

pub use content::*;

pub struct VueCodegen;

impl VueCodegen {
    fn generate_files(name: &str, version: &str) -> Vec<(String, String)> {
        let ctx = Ctx::new(name, version);
        vec![
            ("package.json".into(), package_json(&ctx)),
            ("index.html".into(), index_html(&ctx)),
            ("tsconfig.json".into(), tsconfig_json()),
            ("tsconfig.app.json".into(), tsconfig_app_json()),
            ("tsconfig.node.json".into(), tsconfig_node_json()),
            ("vite.config.ts".into(), vite_config_ts()),
            ("eslint.config.mjs".into(), eslint_config()),
            ("env.d.ts".into(), env_dts()),
            (".gitignore".into(), gitignore()),
            (".env.example".into(), env_example()),
            ("README.md".into(), readme(&ctx)),
            ("src/main.ts".into(), main_ts()),
            ("src/App.vue".into(), app_vue()),
            ("src/pages/HomePage.vue".into(), home_page_vue(&ctx)),
            ("src/pages/AboutPage.vue".into(), about_page_vue(&ctx)),
            ("src/components/ui/AppButton.vue".into(), app_button_vue()),
            ("src/components/ui/AppInput.vue".into(), app_input_vue()),
            ("src/components/ui/AppCard.vue".into(), app_card_vue()),
            ("src/components/features/AppHeader.vue".into(), app_header_vue(&ctx)),
            ("src/composables/useAuth.ts".into(), use_auth_ts()),
            ("src/router/index.ts".into(), router_index_ts()),
            ("src/services/api.ts".into(), api_ts()),
            ("src/stores/auth.ts".into(), auth_store_ts()),
            ("src/types/index.ts".into(), types_index_ts()),
            ("src/utils/helpers.ts".into(), helpers_ts()),
            ("src/styles/main.css".into(), main_css()),
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

impl ScaffoldEngine for VueCodegen {
    fn name(&self) -> &str { "vue" }

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
        name: "vue",
        description: "Vue 3 SPA with Vite, Pinia, and Router",
        commands: &["vue"],
        supported_flags: &["typescript", "tailwindcss"],
        create_engine: || -> Box<dyn ScaffoldEngine> { Box::new(VueCodegen) },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::versions::*;

    #[test]
    fn test_vue_generate_content() {
        let ctx = Ctx::new("my-vue-app", "1.0.0");
        let pkg = package_json(&ctx);
        assert!(pkg.contains("vue"));
        assert!(pkg.contains("my-vue-app"));
        assert!(pkg.contains(VUE()));
        assert!(pkg.contains(VUE_ROUTER()));

        let html = index_html(&ctx);
        assert!(html.contains("my-vue-app"));

        let readme_content = readme(&ctx);
        assert!(readme_content.contains("MyVueApp"));
    }

    #[test]
    fn test_vue_codegen_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        let engine = VueCodegen;
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-vue-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-vue-app", dest.clone()).with_vars(vars);
        let result = engine.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 25);
        assert!(dest.join("package.json").exists());
        assert!(dest.join("src/main.ts").exists());
        assert!(dest.join("src/App.vue").exists());
        assert!(dest.join("src/pages/HomePage.vue").exists());
    }

    #[test]
    fn test_vue_readme_pascal_case() {
        let ctx = Ctx::new("my-app", "1.0.0");
        let readme_content = readme(&ctx);
        assert!(readme_content.contains("MyApp"));
    }

    #[test]
    fn test_vue_engine_name() {
        let engine = VueCodegen;
        assert_eq!(engine.name(), "vue");
    }

    #[test]
    fn test_vue_template() {
        let tpl = template();
        assert_eq!(tpl.name, "vue");
        assert_eq!(tpl.commands, &["vue"]);
        let engine = (tpl.create_engine)();
        assert_eq!(engine.name(), "vue");
    }

    #[test]
    fn test_vue_files_content() {
        let files = VueCodegen::generate_files("test-project", "2.0.0");
        let file_map: std::collections::HashMap<&str, &str> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        assert!(file_map.contains_key("package.json"));
        assert!(file_map.contains_key("src/App.vue"));
        assert!(file_map.contains_key("src/pages/HomePage.vue"));
        assert!(file_map.contains_key("src/components/ui/AppButton.vue"));
        assert!(file_map.contains_key("src/stores/auth.ts"));

        let pkg = file_map.get("package.json").unwrap();
        assert!(pkg.contains("vue"));
        assert!(pkg.contains("test-project"));
        assert!(pkg.contains("2.0.0"));

        let home = file_map.get("src/pages/HomePage.vue").unwrap();
        assert!(home.contains("test-project"));

        let about = file_map.get("src/pages/AboutPage.vue").unwrap();
        assert!(about.contains("test-project"));
    }
}
