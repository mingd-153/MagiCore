use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("vanilla/package.json.hbs")),
    (
        "tsconfig.json.hbs",
        include_str!("vanilla/tsconfig.json.hbs"),
    ),
    (
        "tsconfig.node.json.hbs",
        include_str!("vanilla/tsconfig.node.json.hbs"),
    ),
    (
        "vite.config.ts.hbs",
        include_str!("vanilla/vite.config.ts.hbs"),
    ),
    (
        "src/index.html.hbs",
        include_str!("vanilla/src/index.html.hbs"),
    ),
    ("src/main.ts.hbs", include_str!("vanilla/src/main.ts.hbs")),
    (
        "src/styles/main.css.hbs",
        include_str!("vanilla/src/styles/main.css.hbs"),
    ),
    (
        "src/components/app.ts.hbs",
        include_str!("vanilla/src/components/app.ts.hbs"),
    ),
    (
        "src/utils/helpers.ts.hbs",
        include_str!("vanilla/src/utils/helpers.ts.hbs"),
    ),
    (".gitignore.hbs", include_str!("vanilla/.gitignore.hbs")),
    (".env.example.hbs", include_str!("vanilla/.env.example.hbs")),
    (
        ".editorconfig.hbs",
        include_str!("vanilla/.editorconfig.hbs"),
    ),
    ("README.md.hbs", include_str!("vanilla/README.md.hbs")),
    ("public/.gitkeep", include_str!("vanilla/public/.gitkeep")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct VanillaScaffolder(StaticScaffolder);

impl ScaffoldEngine for VanillaScaffolder {
    fn name(&self) -> &str {
        "vanilla"
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
    let base = std::env::temp_dir().join("mgpm-vanilla");
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
    Box::new(VanillaScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "vanilla",
        description: "Vanilla JS/TS web app with Vite",
        commands: &["web"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_vanilla_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("tsconfig.json.hbs").exists());
        assert!(dir.path().join("src/index.html.hbs").exists());
        assert!(dir.path().join("src/main.ts.hbs").exists());
        assert!(dir.path().join("src/styles/main.css.hbs").exists());
        assert!(dir.path().join("README.md.hbs").exists());
    }

    #[test]
    fn test_vanilla_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-app", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 14);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/main.ts").exists());
    }

    #[test]
    fn test_vanilla_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "vanilla");
    }
}
