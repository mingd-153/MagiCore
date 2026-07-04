mod content;

use std::path::{Path, PathBuf};

use crate::engine::{ProjectCreated, ScaffoldContext, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::templates::{write_favicon, Template};
use crate::validate::NameValidator;

pub use content::*;

pub struct ReactCodegen;

impl ReactCodegen {
    fn generate_files(name: &str, version: &str, has_ts: bool) -> Vec<(String, String)> {
        let ctx = Ctx { name: name.to_string(), version: version.to_string(), has_ts };
        let e = ctx.ext();
        let r = ctx.ext_raw();
        let mut files = Vec::with_capacity(if has_ts { 26 } else { 20 });

        // Common files (both JS and TS mode)
        files.push(("package.json".into(), package_json(&ctx)));
        files.push(("index.html".into(), index_html(&ctx)));
        files.push((".gitignore".into(), gitignore()));
        files.push((".env.example".into(), env_example()));
        files.push(("README.md".into(), readme(&ctx)));
        files.push(("src/styles/globals.css".to_string(), globals_css()));

        let cfg_ext = if has_ts { "ts" } else { "js" };
        files.push((format!("vite.config.{cfg_ext}"), vite_config_js()));

        if has_ts {
            files.push(("eslint.config.mjs".into(), eslint_config()));
            files.push(("tsconfig.json".into(), tsconfig_json()));
            files.push(("tsconfig.app.json".into(), tsconfig_app_json()));
            files.push(("tsconfig.node.json".into(), tsconfig_node_json()));
            files.push(("src/vite-env.d.ts".into(), vite_env_dts()));
        }

        // Entry
        files.push((format!("src/main.{e}"), main_content(&ctx)));
        files.push((format!("src/App.{e}"), app_content()));

        // Pages
        files.push((format!("src/pages/home.{e}"), home_content(name)));
        files.push((format!("src/pages/about.{e}"), about_content(name)));

        // Components
        files.push((format!("src/components/features/header.{e}"), header_content(name)));

        if has_ts {
            files.push((format!("src/components/ui/button.{e}"), button_tsx()));
            files.push((format!("src/components/ui/input.{e}"), input_tsx()));
            files.push((format!("src/components/ui/card.{e}"), card_tsx()));
            files.push((format!("src/hooks/use-auth.{r}"), use_auth_ts()));
            files.push((format!("src/services/api.{r}"), api_ts()));
            files.push((format!("src/stores/auth-store.{r}"), auth_store_ts()));
            files.push((format!("src/types/index.{r}"), types_ts()));
            files.push((format!("src/utils/helpers.{r}"), helpers_ts()));
        } else {
            files.push((format!("src/components/ui/button.{e}"), button_jsx()));
            files.push((format!("src/components/ui/input.{e}"), input_jsx()));
            files.push((format!("src/components/ui/card.{e}"), card_jsx()));
            files.push((format!("src/hooks/use-auth.{r}"), use_auth_js()));
            files.push((format!("src/services/api.{r}"), api_js()));
            files.push((format!("src/stores/auth-store.{r}"), auth_store_js()));
            files.push((format!("src/utils/helpers.{r}"), helpers_js()));
        }

        files
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

impl ScaffoldEngine for ReactCodegen {
    fn name(&self) -> &str { "react" }

    fn create_project(&self, ctx: &ScaffoldContext, force: bool) -> Result<ProjectCreated, ScaffoldError> {
        NameValidator::validate(&ctx.project_name).map_err(|e| {
            ScaffoldError::InvalidName(ctx.project_name.clone(), e.to_string())
        })?;
        let name = &ctx.project_name;
        let version = ctx.get_var("version").unwrap_or("1.0.0");
        let has_ts = ctx.features.iter().any(|f| f == "typescript");
        let dest = Self::resolve_dest(ctx)?;
        let files = Self::generate_files(name, version, has_ts);
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
        name: "react",
        description: "React SPA with Vite, Router, and Zustand",
        commands: &["react"],
        create_engine: || Box::new(ReactCodegen),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_js_default_19_files() {
        let files = ReactCodegen::generate_files("app", "1.0.0", false);
        assert_eq!(files.len(), 19);
    }

    #[test]
    fn test_ts_25_files() {
        let files = ReactCodegen::generate_files("app", "1.0.0", true);
        assert_eq!(files.len(), 25);
    }

    #[test]
    fn test_js_has_no_tsconfig() {
        let files = ReactCodegen::generate_files("app", "1.0.0", false);
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"vite.config.js"));
        assert!(!paths.contains(&"tsconfig.json"));
        assert!(!paths.contains(&"eslint.config.mjs"));
    }

    #[test]
    fn test_ts_has_config_files() {
        let files = ReactCodegen::generate_files("app", "1.0.0", true);
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"vite.config.ts"));
        assert!(paths.contains(&"tsconfig.json"));
        assert!(paths.contains(&"eslint.config.mjs"));
        assert!(paths.contains(&"src/vite-env.d.ts"));
    }

    #[test]
    fn test_js_extensions() {
        let files = ReactCodegen::generate_files("app", "1.0.0", false);
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/main.jsx"));
        assert!(paths.contains(&"src/App.jsx"));
        assert!(paths.contains(&"src/pages/home.jsx"));
        assert!(paths.contains(&"src/components/ui/button.jsx"));
        assert!(paths.contains(&"src/services/api.js"));
        assert!(paths.contains(&"src/hooks/use-auth.js"));
    }

    #[test]
    fn test_ts_extensions() {
        let files = ReactCodegen::generate_files("app", "1.0.0", true);
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/main.tsx"));
        assert!(paths.contains(&"src/App.tsx"));
        assert!(paths.contains(&"src/pages/home.tsx"));
        assert!(paths.contains(&"src/components/ui/button.tsx"));
        assert!(paths.contains(&"src/services/api.ts"));
        assert!(paths.contains(&"src/types/index.ts"));
    }

    #[test]
    fn test_package_json_ts_has_typescript() {
        let pkg = package_json(&Ctx::new("app", "1.0.0", true));
        assert!(pkg.contains("typescript"));
        assert!(pkg.contains("tsc -b"));
        assert!(pkg.contains("@types/react"));
    }

    #[test]
    fn test_package_json_js_no_typescript() {
        let pkg = package_json(&Ctx::new("app", "1.0.0", false));
        assert!(!pkg.contains("typescript"));
        assert!(!pkg.contains("tsc -b"));
        assert!(pkg.contains("vite build"));
    }

    #[test]
    fn test_index_html_ts_script_src() {
        let html = index_html(&Ctx::new("app", "1.0.0", true));
        assert!(html.contains("src/main.tsx"));
    }

    #[test]
    fn test_index_html_js_script_src() {
        let html = index_html(&Ctx::new("app", "1.0.0", false));
        assert!(html.contains("src/main.jsx"));
    }

    #[test]
    fn test_readme_uses_mg_commands() {
        let r = readme(&Ctx::new("app", "1.0.0", false));
        assert!(r.contains("mg install"));
        assert!(r.contains("mg run dev"));
        assert!(!r.contains("npm install"));
        assert!(!r.contains("npm run"));
    }

    #[test]
    fn test_header_no_broken_jsx() {
        let h = header_content("my-app");
        assert!(!h.contains("{'{'"));
        assert!(!h.contains("{'}'}"));
        assert!(h.contains("className={({"));
    }

    #[test]
    fn test_home_contains_name() {
        let content = home_content("my-app");
        assert!(content.contains("Welcome to my-app"));
    }

    #[test]
    fn test_about_contains_name() {
        let content = about_content("my-app");
        assert!(content.contains("About my-app"));
    }

    #[test]
    fn test_engine_creates_js_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        ReactCodegen.create_project(&ctx, false).unwrap();
        assert!(dest.join("package.json").exists());
        assert!(dest.join("vite.config.js").exists());
        assert!(dest.join("src/main.jsx").exists());
        assert!(dest.join("src/pages/home.jsx").exists());
        assert!(dest.join("src/services/api.js").exists());
        assert!(!dest.join("tsconfig.json").exists());
    }

    #[test]
    fn test_engine_creates_ts_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone())
            .with_features(vec!["typescript".into()]);
        ReactCodegen.create_project(&ctx, false).unwrap();
        assert!(dest.join("vite.config.ts").exists());
        assert!(dest.join("src/main.tsx").exists());
        assert!(dest.join("tsconfig.json").exists());
        assert!(dest.join("eslint.config.mjs").exists());
    }

    #[test]
    fn test_engine_content_is_rendered_js() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        ReactCodegen.create_project(&ctx, false).unwrap();
        let html = std::fs::read_to_string(dest.join("index.html")).unwrap();
        assert!(html.contains("my-app"));
        let home = std::fs::read_to_string(dest.join("src/pages/home.jsx")).unwrap();
        assert!(home.contains("Welcome to my-app"));
    }

    #[test]
    fn test_engine_content_is_rendered_ts() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone())
            .with_features(vec!["typescript".into()]);
        ReactCodegen.create_project(&ctx, false).unwrap();
        let html = std::fs::read_to_string(dest.join("index.html")).unwrap();
        assert!(html.contains("my-app"));
        let home = std::fs::read_to_string(dest.join("src/pages/home.tsx")).unwrap();
        assert!(home.contains("Welcome to my-app"));
    }

    #[test]
    fn test_engine_fails_on_existing() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("index.html"), "x").unwrap();
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        let result = ReactCodegen.create_project(&ctx, false);
        assert!(matches!(result, Err(ScaffoldError::PathExists(_))));
    }

    #[test]
    fn test_engine_force_overwrites() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("index.html"), "old").unwrap();
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        ReactCodegen.create_project(&ctx, true).unwrap();
        let c = std::fs::read_to_string(dest.join("index.html")).unwrap();
        assert!(c.contains("my-app"));
        assert!(!c.contains("old"));
    }

    #[test]
    fn test_invalid_name() {
        let ctx = ScaffoldContext::new("", PathBuf::from("/tmp/x"));
        let result = ReactCodegen.create_project(&ctx, false);
        assert!(matches!(result, Err(ScaffoldError::InvalidName(_, _))));
    }

    #[test]
    fn test_readme_pascal_case() {
        let r = readme(&Ctx::new("my-cool-app", "1.0.0", true));
        assert!(r.contains("MyCoolApp"));
    }
}
