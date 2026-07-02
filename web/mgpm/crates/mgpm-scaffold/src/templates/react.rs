use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("react/package.json.hbs")),
    (
        "tsconfig.json.hbs",
        include_str!("react/tsconfig.json.hbs"),
    ),
    (
        "tsconfig.app.json.hbs",
        include_str!("react/tsconfig.app.json.hbs"),
    ),
    (
        "tsconfig.node.json.hbs",
        include_str!("react/tsconfig.node.json.hbs"),
    ),
    (
        "vite.config.ts.hbs",
        include_str!("react/vite.config.ts.hbs"),
    ),
    (
        "eslint.config.mjs.hbs",
        include_str!("react/eslint.config.mjs.hbs"),
    ),
    (
        "index.html.hbs",
        include_str!("react/index.html.hbs"),
    ),
    ("src/main.tsx.hbs", include_str!("react/src/main.tsx.hbs")),
    ("src/App.tsx.hbs", include_str!("react/src/App.tsx.hbs")),
    (
        "src/vite-env.d.ts",
        include_str!("react/src/vite-env.d.ts"),
    ),
    (
        "src/pages/home.tsx.hbs",
        include_str!("react/src/pages/home.tsx.hbs"),
    ),
    (
        "src/pages/about.tsx.hbs",
        include_str!("react/src/pages/about.tsx.hbs"),
    ),
    (
        "src/components/ui/button.tsx.hbs",
        include_str!("react/src/components/ui/button.tsx.hbs"),
    ),
    (
        "src/components/ui/input.tsx.hbs",
        include_str!("react/src/components/ui/input.tsx.hbs"),
    ),
    (
        "src/components/ui/card.tsx.hbs",
        include_str!("react/src/components/ui/card.tsx.hbs"),
    ),
    (
        "src/components/features/header.tsx.hbs",
        include_str!("react/src/components/features/header.tsx.hbs"),
    ),
    (
        "src/hooks/use-auth.ts.hbs",
        include_str!("react/src/hooks/use-auth.ts.hbs"),
    ),
    (
        "src/services/api.ts.hbs",
        include_str!("react/src/services/api.ts.hbs"),
    ),
    (
        "src/stores/auth-store.ts.hbs",
        include_str!("react/src/stores/auth-store.ts.hbs"),
    ),
    (
        "src/types/index.ts.hbs",
        include_str!("react/src/types/index.ts.hbs"),
    ),
    (
        "src/utils/helpers.ts.hbs",
        include_str!("react/src/utils/helpers.ts.hbs"),
    ),
    (
        "src/styles/globals.css.hbs",
        include_str!("react/src/styles/globals.css.hbs"),
    ),
    (".gitignore.hbs", include_str!("react/.gitignore.hbs")),
    (".env.example.hbs", include_str!("react/.env.example.hbs")),
    ("README.md.hbs", include_str!("react/README.md.hbs")),
    ("public/.gitkeep", include_str!("react/public/.gitkeep")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct ReactScaffolder(StaticScaffolder);

impl ScaffoldEngine for ReactScaffolder {
    fn name(&self) -> &str {
        "react"
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
    let base = std::env::temp_dir().join("mgpm-react");
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
    Box::new(ReactScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "react",
        description: "React SPA with Vite, Router, and Zustand",
        commands: &["react"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_react_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("tsconfig.app.json.hbs").exists());
        assert!(dir.path().join("src/main.tsx.hbs").exists());
        assert!(dir.path().join("src/App.tsx.hbs").exists());
        assert!(dir.path().join("src/pages/home.tsx.hbs").exists());
        assert!(dir.path().join("src/components/ui/button.tsx.hbs").exists());
        assert!(dir.path().join("src/stores/auth-store.ts.hbs").exists());
        assert!(dir.path().join("eslint.config.mjs.hbs").exists());
    }

    #[test]
    fn test_react_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-react-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-react-app", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 20);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/main.tsx").exists());
        assert!(temp.path().join("output/src/App.tsx").exists());
    }

    #[test]
    fn test_react_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "react");
    }
}
