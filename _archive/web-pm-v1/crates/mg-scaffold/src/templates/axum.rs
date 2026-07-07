use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("Cargo.toml.hbs", include_str!("axum/Cargo.toml.hbs")),
    (".env.example.hbs", include_str!("axum/.env.example.hbs")),
    ("src/main.rs.hbs", include_str!("axum/src/main.rs.hbs")),
    (
        "src/models/mod.rs.hbs",
        include_str!("axum/src/models/mod.rs.hbs"),
    ),
    (
        "src/models/item.rs.hbs",
        include_str!("axum/src/models/item.rs.hbs"),
    ),
    (
        "src/handlers/mod.rs.hbs",
        include_str!("axum/src/handlers/mod.rs.hbs"),
    ),
    (
        "src/handlers/items.rs.hbs",
        include_str!("axum/src/handlers/items.rs.hbs"),
    ),
    (
        "migrations/20240101000001_create_items.sql.hbs",
        include_str!("axum/migrations/20240101000001_create_items.sql.hbs"),
    ),
    ("Dockerfile.hbs", include_str!("axum/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("axum/.gitignore.hbs")),
    ("README.md.hbs", include_str!("axum/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct AxumScaffolder(StaticScaffolder);

impl ScaffoldEngine for AxumScaffolder {
    fn name(&self) -> &str {
        "axum"
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
    let base = std::env::temp_dir().join("mg-axum");
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
    Box::new(AxumScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "axum",
        description: "Axum REST API with Rust",
        commands: &["axum"],
    supported_flags: &[],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_axum_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("Cargo.toml.hbs").exists());
        assert!(dir.path().join("src/main.rs.hbs").exists());
        assert!(dir.path().join("src/models/item.rs.hbs").exists());
        assert!(dir.path().join("src/handlers/items.rs.hbs").exists());
        assert!(dir.path().join("migrations/20240101000001_create_items.sql.hbs").exists());
        assert!(dir.path().join("Dockerfile.hbs").exists());
        assert!(dir.path().join(".gitignore.hbs").exists());
        assert!(dir.path().join(".env.example.hbs").exists());
        assert!(dir.path().join("README.md.hbs").exists());
    }

    #[test]
    fn test_axum_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-axum-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-axum-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 11);
        assert!(temp.path().join("output/Cargo.toml").exists());
        assert!(temp.path().join("output/src/main.rs").exists());
        assert!(temp.path().join("output/src/handlers/items.rs").exists());
    }

    #[test]
    fn test_axum_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "axum");
    }
}
