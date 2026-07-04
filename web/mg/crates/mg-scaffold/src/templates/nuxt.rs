use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, write_favicon, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("nuxt/package.json.hbs")),
    ("nuxt.config.ts.hbs", include_str!("nuxt/nuxt.config.ts.hbs")),
    ("tsconfig.json.hbs", include_str!("nuxt/tsconfig.json.hbs")),
    ("app.vue.hbs", include_str!("nuxt/app.vue.hbs")),
    ("pages/index.vue.hbs", include_str!("nuxt/pages/index.vue.hbs")),
    ("pages/about.vue.hbs", include_str!("nuxt/pages/about.vue.hbs")),
    ("components/Header.vue.hbs", include_str!("nuxt/components/Header.vue.hbs")),
    ("composables/useAuth.ts.hbs", include_str!("nuxt/composables/useAuth.ts.hbs")),
    ("stores/auth.ts.hbs", include_str!("nuxt/stores/auth.ts.hbs")),
    ("services/api.ts.hbs", include_str!("nuxt/services/api.ts.hbs")),
    ("types/index.ts.hbs", include_str!("nuxt/types/index.ts.hbs")),
    ("utils/helpers.ts.hbs", include_str!("nuxt/utils/helpers.ts.hbs")),
    ("assets/css/main.css.hbs", include_str!("nuxt/assets/css/main.css.hbs")),
    (".gitignore.hbs", include_str!("nuxt/.gitignore.hbs")),
    (".env.example.hbs", include_str!("nuxt/.env.example.hbs")),
    ("README.md.hbs", include_str!("nuxt/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct NuxtScaffolder(StaticScaffolder);

impl ScaffoldEngine for NuxtScaffolder {
    fn name(&self) -> &str {
        "nuxt"
    }

    fn create_project(
        &self,
        ctx: &ScaffoldContext,
        force: bool,
    ) -> Result<ProjectCreated, ScaffoldError> {
        let result = self.0.create_project(ctx, force)?;
        write_favicon(&result.path)?;
        Ok(result)
    }
}

fn create_temp_dir() -> Result<PathBuf, ScaffoldError> {
    let base = std::env::temp_dir().join("mg-nuxt");
    let id = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = base.join(id.to_string());
    std::fs::create_dir_all(&path).map_err(|e| ScaffoldError::IoError {
        context: "create temp template dir".to_string(),
        source: e,
    })?;
    Ok(path)
}

fn build_engine() -> Box<dyn ScaffoldEngine> {
    let path = create_temp_dir().expect("create temp dir for template extraction");
    let map: HashMap<&str, &str> = FILES.iter().copied().collect();
    extract_embedded(&path, &map).expect("extract embedded templates");
    Box::new(NuxtScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "nuxt",
        description: "Nuxt 3 Vue web app with TypeScript",
        commands: &["nuxt"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_nuxt_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("nuxt.config.ts.hbs").exists());
        assert!(dir.path().join("app.vue.hbs").exists());
        assert!(dir.path().join("pages/index.vue.hbs").exists());
    }

    #[test]
    fn test_nuxt_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-nuxt-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-nuxt-app", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 14);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/app.vue").exists());
        assert!(temp.path().join("output/pages/index.vue").exists());
        assert!(temp.path().join("output/nuxt.config.ts").exists());
    }

    #[test]
    fn test_nuxt_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "nuxt");
    }
}
