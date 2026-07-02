use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("vue/package.json.hbs")),
    (
        "index.html.hbs",
        include_str!("vue/index.html.hbs"),
    ),
    (
        "tsconfig.json.hbs",
        include_str!("vue/tsconfig.json.hbs"),
    ),
    (
        "tsconfig.app.json.hbs",
        include_str!("vue/tsconfig.app.json.hbs"),
    ),
    (
        "tsconfig.node.json.hbs",
        include_str!("vue/tsconfig.node.json.hbs"),
    ),
    (
        "vite.config.ts.hbs",
        include_str!("vue/vite.config.ts.hbs"),
    ),
    (
        "eslint.config.mjs.hbs",
        include_str!("vue/eslint.config.mjs.hbs"),
    ),
    (
        "env.d.ts",
        include_str!("vue/env.d.ts"),
    ),
    ("src/main.ts.hbs", include_str!("vue/src/main.ts.hbs")),
    ("src/App.vue.hbs", include_str!("vue/src/App.vue.hbs")),
    (
        "src/pages/HomePage.vue.hbs",
        include_str!("vue/src/pages/HomePage.vue.hbs"),
    ),
    (
        "src/pages/AboutPage.vue.hbs",
        include_str!("vue/src/pages/AboutPage.vue.hbs"),
    ),
    (
        "src/components/ui/AppButton.vue.hbs",
        include_str!("vue/src/components/ui/AppButton.vue.hbs"),
    ),
    (
        "src/components/ui/AppInput.vue.hbs",
        include_str!("vue/src/components/ui/AppInput.vue.hbs"),
    ),
    (
        "src/components/ui/AppCard.vue.hbs",
        include_str!("vue/src/components/ui/AppCard.vue.hbs"),
    ),
    (
        "src/components/features/AppHeader.vue.hbs",
        include_str!("vue/src/components/features/AppHeader.vue.hbs"),
    ),
    (
        "src/composables/useAuth.ts.hbs",
        include_str!("vue/src/composables/useAuth.ts.hbs"),
    ),
    (
        "src/router/index.ts.hbs",
        include_str!("vue/src/router/index.ts.hbs"),
    ),
    (
        "src/services/api.ts.hbs",
        include_str!("vue/src/services/api.ts.hbs"),
    ),
    (
        "src/stores/auth.ts.hbs",
        include_str!("vue/src/stores/auth.ts.hbs"),
    ),
    (
        "src/types/index.ts.hbs",
        include_str!("vue/src/types/index.ts.hbs"),
    ),
    (
        "src/utils/helpers.ts.hbs",
        include_str!("vue/src/utils/helpers.ts.hbs"),
    ),
    (
        "src/styles/main.css.hbs",
        include_str!("vue/src/styles/main.css.hbs"),
    ),
    (".gitignore.hbs", include_str!("vue/.gitignore.hbs")),
    (".env.example.hbs", include_str!("vue/.env.example.hbs")),
    ("README.md.hbs", include_str!("vue/README.md.hbs")),
    ("public/.gitkeep", include_str!("vue/public/.gitkeep")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct VueScaffolder(StaticScaffolder);

impl ScaffoldEngine for VueScaffolder {
    fn name(&self) -> &str {
        "vue"
    }

    fn create_project(
        &self,
        ctx: &ScaffoldContext,
        force: bool,
    ) -> Result<ProjectCreated, ScaffoldError> {
        self.0.create_project(ctx, force)
    }
}

fn create_temp_dir() -> Result<PathBuf, ScaffoldError> {
    let base = std::env::temp_dir().join("mgpm-vue");
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
    Box::new(VueScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "vue",
        description: "Vue 3 SPA with Vite, Pinia, and Router",
        commands: &["vue"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_vue_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("src/main.ts.hbs").exists());
        assert!(dir.path().join("src/App.vue.hbs").exists());
        assert!(dir.path().join("src/pages/HomePage.vue.hbs").exists());
        assert!(dir.path().join("src/components/ui/AppButton.vue.hbs").exists());
        assert!(dir.path().join("src/stores/auth.ts.hbs").exists());
    }

    #[test]
    fn test_vue_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-vue-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-vue-app", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 20);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/main.ts").exists());
        assert!(temp.path().join("output/src/App.vue").exists());
    }

    #[test]
    fn test_vue_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "vue");
    }
}