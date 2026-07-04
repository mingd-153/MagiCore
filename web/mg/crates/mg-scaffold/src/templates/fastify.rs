use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    (
        "package.json.hbs",
        include_str!("fastify/package.json.hbs"),
    ),
    (
        "tsconfig.json.hbs",
        include_str!("fastify/tsconfig.json.hbs"),
    ),
    (
        "src/index.ts.hbs",
        include_str!("fastify/src/index.ts.hbs"),
    ),
    ("src/app.ts.hbs", include_str!("fastify/src/app.ts.hbs")),
    (
        "src/routes/items.ts.hbs",
        include_str!("fastify/src/routes/items.ts.hbs"),
    ),
    (
        "src/types/index.ts.hbs",
        include_str!("fastify/src/types/index.ts.hbs"),
    ),
    ("Dockerfile.hbs", include_str!("fastify/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("fastify/.gitignore.hbs")),
    (
        ".env.example.hbs",
        include_str!("fastify/.env.example.hbs"),
    ),
    ("README.md.hbs", include_str!("fastify/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct FastifyScaffolder(StaticScaffolder);

impl ScaffoldEngine for FastifyScaffolder {
    fn name(&self) -> &str {
        "fastify"
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
    let base = std::env::temp_dir().join("mg-fastify");
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
    Box::new(FastifyScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "fastify",
        description: "Fastify REST API with TypeScript",
        commands: &["fastify"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fastify_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("src/index.ts.hbs").exists());
        assert!(dir.path().join("src/routes/items.ts.hbs").exists());
    }

    #[test]
    fn test_fastify_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-fastify-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-fastify-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 8);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/app.ts").exists());
        assert!(temp.path().join("output/src/routes/items.ts").exists());
    }

    #[test]
    fn test_fastify_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "fastify");
    }
}
