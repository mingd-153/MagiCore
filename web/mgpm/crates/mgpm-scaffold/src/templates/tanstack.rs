use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("tanstack/package.json.hbs")),
    ("app.config.ts.hbs", include_str!("tanstack/app.config.ts.hbs")),
    ("tsconfig.json.hbs", include_str!("tanstack/tsconfig.json.hbs")),
    ("src/routes/__root.tsx.hbs", include_str!("tanstack/src/routes/__root.tsx.hbs")),
    ("src/routes/index.tsx.hbs", include_str!("tanstack/src/routes/index.tsx.hbs")),
    ("src/routes/about.tsx.hbs", include_str!("tanstack/src/routes/about.tsx.hbs")),
    ("src/components/Header.tsx.hbs", include_str!("tanstack/src/components/Header.tsx.hbs")),
    ("src/lib/api.ts.hbs", include_str!("tanstack/src/lib/api.ts.hbs")),
    ("src/styles/global.css.hbs", include_str!("tanstack/src/styles/global.css.hbs")),
    (".gitignore.hbs", include_str!("tanstack/.gitignore.hbs")),
    (".env.example.hbs", include_str!("tanstack/.env.example.hbs")),
    ("README.md.hbs", include_str!("tanstack/README.md.hbs")),
    ("public/favicon.svg.hbs", include_str!("tanstack/public/favicon.svg.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct TanStackScaffolder(StaticScaffolder);

impl ScaffoldEngine for TanStackScaffolder {
    fn name(&self) -> &str {
        "tanstack"
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
    let base = std::env::temp_dir().join("mgpm-tanstack");
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
    Box::new(TanStackScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "tanstack",
        description: "TanStack Start full-stack React app with file-based routing",
        commands: &["tanstack"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tanstack_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("app.config.ts.hbs").exists());
        assert!(dir.path().join("src/routes/__root.tsx.hbs").exists());
        assert!(dir.path().join("src/routes/index.tsx.hbs").exists());
        assert!(dir.path().join("src/components/Header.tsx.hbs").exists());
        assert!(dir.path().join("src/lib/api.ts.hbs").exists());
        assert!(dir.path().join("src/styles/global.css.hbs").exists());
    }

    #[test]
    fn test_tanstack_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-tanstack-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-tanstack-app", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 10);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/routes/index.tsx").exists());
    }

    #[test]
    fn test_tanstack_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "tanstack");
    }
}
