use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("nestjs/package.json.hbs")),
    ("tsconfig.json.hbs", include_str!("nestjs/tsconfig.json.hbs")),
    ("nest-cli.json.hbs", include_str!("nestjs/nest-cli.json.hbs")),
    ("src/main.ts.hbs", include_str!("nestjs/src/main.ts.hbs")),
    (
        "src/app.module.ts.hbs",
        include_str!("nestjs/src/app.module.ts.hbs"),
    ),
    (
        "src/app.controller.ts.hbs",
        include_str!("nestjs/src/app.controller.ts.hbs"),
    ),
    (
        "src/app.service.ts.hbs",
        include_str!("nestjs/src/app.service.ts.hbs"),
    ),
    (
        "src/items/items.module.ts.hbs",
        include_str!("nestjs/src/items/items.module.ts.hbs"),
    ),
    (
        "src/items/items.controller.ts.hbs",
        include_str!("nestjs/src/items/items.controller.ts.hbs"),
    ),
    (
        "src/items/items.service.ts.hbs",
        include_str!("nestjs/src/items/items.service.ts.hbs"),
    ),
    (
        "src/items/dto/create-item.dto.ts.hbs",
        include_str!("nestjs/src/items/dto/create-item.dto.ts.hbs"),
    ),
    (
        "prisma/schema.prisma.hbs",
        include_str!("nestjs/prisma/schema.prisma.hbs"),
    ),
    ("Dockerfile.hbs", include_str!("nestjs/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("nestjs/.gitignore.hbs")),
    (
        ".env.example.hbs",
        include_str!("nestjs/.env.example.hbs"),
    ),
    ("README.md.hbs", include_str!("nestjs/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct NestjsScaffolder(StaticScaffolder);

impl ScaffoldEngine for NestjsScaffolder {
    fn name(&self) -> &str {
        "nestjs"
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
    let base = std::env::temp_dir().join("mgpm-nestjs");
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
    Box::new(NestjsScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "nestjs",
        description: "NestJS API with TypeScript",
        commands: &["nestjs"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_nestjs_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("tsconfig.json.hbs").exists());
        assert!(dir.path().join("nest-cli.json.hbs").exists());
        assert!(dir.path().join("src/main.ts.hbs").exists());
        assert!(dir.path().join("src/app.module.ts.hbs").exists());
        assert!(dir.path().join("src/app.controller.ts.hbs").exists());
        assert!(dir.path().join("src/app.service.ts.hbs").exists());
        assert!(dir.path().join("src/items/items.module.ts.hbs").exists());
        assert!(dir.path().join("src/items/items.controller.ts.hbs").exists());
        assert!(dir.path().join("src/items/items.service.ts.hbs").exists());
        assert!(dir.path().join("src/items/dto/create-item.dto.ts.hbs").exists());
        assert!(dir.path().join("prisma/schema.prisma.hbs").exists());
        assert!(dir.path().join("Dockerfile.hbs").exists());
        assert!(dir.path().join(".gitignore.hbs").exists());
        assert!(dir.path().join(".env.example.hbs").exists());
        assert!(dir.path().join("README.md.hbs").exists());
    }

    #[test]
    fn test_nestjs_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new("my-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 16);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/main.ts").exists());
        assert!(temp.path().join("output/src/app.module.ts").exists());
        assert!(temp.path().join("output/prisma/schema.prisma").exists());
    }

    #[test]
    fn test_nestjs_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "nestjs");
    }
}
