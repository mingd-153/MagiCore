mod content;

use std::path::{Path, PathBuf};

use crate::engine::{ProjectCreated, ScaffoldContext, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::templates::{write_favicon, Template};
use crate::validate::NameValidator;

pub use content::*;

pub struct NextCodegen;

impl NextCodegen {
    fn generate_files(name: &str, version: &str) -> Vec<(String, String)> {
        let ctx = Ctx::new(name, version);
        vec![
            ("package.json".into(), package_json(&ctx)),
            ("next.config.ts".into(), next_config()),
            ("tsconfig.json".into(), tsconfig_json()),
            ("postcss.config.mjs".into(), postcss_config()),
            ("tailwind.config.ts".into(), tailwind_config()),
            ("eslint.config.mjs".into(), eslint_config()),
            (".gitignore".into(), gitignore()),
            (".env.example".into(), env_example()),
            (".env.local.example".into(), env_local_example()),
            ("README.md".into(), readme(&ctx)),
            ("Dockerfile".into(), dockerfile()),
            (".github/workflows/ci.yml".into(), ci_yml()),
            ("src/app/layout.tsx".into(), root_layout(&ctx)),
            ("src/app/page.tsx".into(), home_page(&ctx)),
            ("src/app/not-found.tsx".into(), not_found()),
            ("src/app/globals.css".into(), globals_css()),
            ("src/app/(marketing)/about/page.tsx".into(), about_page(&ctx)),
            ("src/app/(dashboard)/layout.tsx".into(), dashboard_layout()),
            ("src/app/(dashboard)/page.tsx".into(), dashboard_page()),
            ("src/app/api/hello/route.ts".into(), api_hello(&ctx)),
            ("src/app/api/auth/route.ts".into(), api_auth()),
            ("src/actions/auth.ts".into(), auth_actions()),
            ("src/components/ui/button.tsx".into(), button()),
            ("src/components/ui/input.tsx".into(), input_component()),
            ("src/components/ui/card.tsx".into(), card()),
            ("src/components/features/header.tsx".into(), header(&ctx)),
            ("src/components/features/footer.tsx".into(), footer(&ctx)),
            ("src/lib/utils.ts".into(), utils()),
            ("src/lib/db.ts".into(), db_stub()),
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

impl ScaffoldEngine for NextCodegen {
    fn name(&self) -> &str { "next" }

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
        name: "next-app",
        description: "Next.js fullstack app with App Router + TypeScript + Tailwind CSS",
        commands: &["next-app", "next"],
        supported_flags: &["typescript", "tailwindcss"],
        create_engine: || Box::new(NextCodegen),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_count() {
        let files = NextCodegen::generate_files("app", "1.0.0");
        assert_eq!(files.len(), 29);
    }

    #[test]
    fn test_all_paths_exist() {
        let files = NextCodegen::generate_files("app", "1.0.0");
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"package.json"));
        assert!(paths.contains(&"next.config.ts"));
        assert!(paths.contains(&"tsconfig.json"));
        assert!(paths.contains(&"postcss.config.mjs"));
        assert!(paths.contains(&"tailwind.config.ts"));
        assert!(paths.contains(&"eslint.config.mjs"));
        assert!(paths.contains(&".gitignore"));
        assert!(paths.contains(&".env.example"));
        assert!(paths.contains(&"README.md"));
        assert!(paths.contains(&"Dockerfile"));
        assert!(paths.contains(&".github/workflows/ci.yml"));
        assert!(paths.contains(&"src/app/layout.tsx"));
        assert!(paths.contains(&"src/app/page.tsx"));
        assert!(paths.contains(&"src/app/not-found.tsx"));
        assert!(paths.contains(&"src/app/globals.css"));
        assert!(paths.contains(&"src/app/(marketing)/about/page.tsx"));
        assert!(paths.contains(&"src/app/(dashboard)/layout.tsx"));
        assert!(paths.contains(&"src/app/(dashboard)/page.tsx"));
        assert!(paths.contains(&"src/app/api/hello/route.ts"));
        assert!(paths.contains(&"src/app/api/auth/route.ts"));
        assert!(paths.contains(&"src/actions/auth.ts"));
        assert!(paths.contains(&"src/components/ui/button.tsx"));
        assert!(paths.contains(&"src/components/ui/input.tsx"));
        assert!(paths.contains(&"src/components/ui/card.tsx"));
        assert!(paths.contains(&"src/components/features/header.tsx"));
        assert!(paths.contains(&"src/components/features/footer.tsx"));
        assert!(paths.contains(&"src/lib/utils.ts"));
        assert!(paths.contains(&"src/lib/db.ts"));
    }

    #[test]
    fn test_package_json_has_all_deps() {
        let pkg = package_json(&Ctx::new("app", "1.0.0"));
        assert!(pkg.contains("next"));
        assert!(pkg.contains("react"));
        assert!(pkg.contains("react-dom"));
        assert!(pkg.contains("typescript"));
        assert!(pkg.contains("@types/node"));
        assert!(pkg.contains("@types/react"));
        assert!(pkg.contains("@types/react-dom"));
        assert!(pkg.contains("tailwindcss"));
        assert!(pkg.contains("@tailwindcss/postcss"));
        assert!(pkg.contains("eslint"));
        assert!(pkg.contains("@eslint/js"));
        assert!(pkg.contains("typescript-eslint"));
        assert!(pkg.contains("@next/eslint-plugin-next"));
        assert!(pkg.contains("prettier"));
        assert!(pkg.contains("prettier-plugin-tailwindcss"));
    }

    #[test]
    fn test_package_json_name_and_version() {
        let pkg = package_json(&Ctx::new("my-app", "2.0.0"));
        assert!(pkg.contains("\"name\":\"my-app\""));
        assert!(pkg.contains("\"version\":\"2.0.0\""));
    }

    #[test]
    fn test_root_layout_contains_name() {
        let layout = root_layout(&Ctx::new("my-app", "1.0.0"));
        assert!(layout.contains("title: 'my-app'"));
    }

    #[test]
    fn test_home_page_contains_name() {
        let page = home_page(&Ctx::new("my-app", "1.0.0"));
        assert!(page.contains("Welcome to my-app"));
    }

    #[test]
    fn test_about_page_contains_name() {
        let page = about_page(&Ctx::new("my-app", "1.0.0"));
        assert!(page.contains("About my-app"));
    }

    #[test]
    fn test_header_contains_name() {
        let h = header(&Ctx::new("my-app", "1.0.0"));
        assert!(h.contains("my-app"));
    }

    #[test]
    fn test_footer_contains_name() {
        let f = footer(&Ctx::new("my-app", "1.0.0"));
        assert!(f.contains("my-app"));
    }

    #[test]
    fn test_api_hello_contains_name() {
        let route = api_hello(&Ctx::new("my-app", "1.0.0"));
        assert!(route.contains("Hello from my-app!"));
    }

    #[test]
    fn test_globals_css_has_tailwind_import() {
        let css = globals_css();
        assert!(css.contains("@import 'tailwindcss'"));
    }

    #[test]
    fn test_readme_pascal_case() {
        let r = readme(&Ctx::new("my-cool-app", "1.0.0"));
        assert!(r.contains("MyCoolApp"));
        assert!(r.contains("npm install"));
        assert!(r.contains("npm run dev"));
    }

    #[test]
    fn test_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        NextCodegen.create_project(&ctx, false).unwrap();
        assert!(dest.join("package.json").exists());
        assert!(dest.join("next.config.ts").exists());
        assert!(dest.join("tsconfig.json").exists());
        assert!(dest.join("postcss.config.mjs").exists());
        assert!(dest.join("eslint.config.mjs").exists());
        assert!(dest.join(".gitignore").exists());
        assert!(dest.join("README.md").exists());
        assert!(dest.join("Dockerfile").exists());
        assert!(dest.join("src/app/layout.tsx").exists());
        assert!(dest.join("src/app/page.tsx").exists());
        assert!(dest.join("src/app/globals.css").exists());
        assert!(dest.join("src/components/ui/button.tsx").exists());
        assert!(dest.join("src/lib/utils.ts").exists());
        assert!(dest.join("public/favicon.ico").exists());
    }

    #[test]
    fn test_engine_content_is_rendered() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        NextCodegen.create_project(&ctx, false).unwrap();
        let layout = std::fs::read_to_string(dest.join("src/app/layout.tsx")).unwrap();
        assert!(layout.contains("title: 'my-app'"));
        let home = std::fs::read_to_string(dest.join("src/app/page.tsx")).unwrap();
        assert!(home.contains("Welcome to my-app"));
        let about = std::fs::read_to_string(dest.join("src/app/(marketing)/about/page.tsx")).unwrap();
        assert!(about.contains("About my-app"));
        let hello = std::fs::read_to_string(dest.join("src/app/api/hello/route.ts")).unwrap();
        assert!(hello.contains("Hello from my-app!"));
    }

    #[test]
    fn test_engine_fails_on_existing() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("package.json"), "x").unwrap();
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        let result = NextCodegen.create_project(&ctx, false);
        assert!(matches!(result, Err(ScaffoldError::PathExists(_))));
    }

    #[test]
    fn test_engine_force_overwrites() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("package.json"), "old").unwrap();
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        NextCodegen.create_project(&ctx, true).unwrap();
        let c = std::fs::read_to_string(dest.join("package.json")).unwrap();
        assert!(c.contains("my-app"));
        assert!(!c.contains("old"));
    }

    #[test]
    fn test_invalid_name() {
        let ctx = ScaffoldContext::new("", PathBuf::from("/tmp/x"));
        let result = NextCodegen.create_project(&ctx, false);
        assert!(matches!(result, Err(ScaffoldError::InvalidName(_, _))));
    }

    #[test]
    fn test_dashboard_structure() {
        let files = NextCodegen::generate_files("app", "1.0.0");
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/app/(dashboard)/layout.tsx"));
        assert!(paths.contains(&"src/app/(dashboard)/page.tsx"));
    }

    #[test]
    fn test_actions_and_lib() {
        let files = NextCodegen::generate_files("app", "1.0.0");
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/actions/auth.ts"));
        assert!(paths.contains(&"src/lib/utils.ts"));
        assert!(paths.contains(&"src/lib/db.ts"));
    }

    #[test]
    fn test_static_files() {
        let files = NextCodegen::generate_files("app", "1.0.0");
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&".env.example"));
        assert!(paths.contains(&".env.local.example"));
        assert!(paths.contains(&"Dockerfile"));
    }
}
