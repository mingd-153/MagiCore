use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("go.mod.hbs", include_str!("go-gin/go.mod.hbs")),
    ("cmd/server/main.go.hbs", include_str!("go-gin/cmd/server/main.go.hbs")),
    ("internal/router/router.go.hbs", include_str!("go-gin/internal/router/router.go.hbs")),
    ("internal/handler/item.go.hbs", include_str!("go-gin/internal/handler/item.go.hbs")),
    ("internal/model/item.go.hbs", include_str!("go-gin/internal/model/item.go.hbs")),
    ("internal/repository/item.go.hbs", include_str!("go-gin/internal/repository/item.go.hbs")),
    ("Dockerfile.hbs", include_str!("go-gin/Dockerfile.hbs")),
    ("go.sum.hbs", include_str!("go-gin/go.sum.hbs")),
    (".gitignore.hbs", include_str!("go-gin/.gitignore.hbs")),
    (".env.example.hbs", include_str!("go-gin/.env.example.hbs")),
    ("README.md.hbs", include_str!("go-gin/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct GoGinScaffolder(StaticScaffolder);

impl ScaffoldEngine for GoGinScaffolder {
    fn name(&self) -> &str {
        "go-gin"
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
    let base = std::env::temp_dir().join("mgpm-go-gin");
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
    Box::new(GoGinScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "go-gin",
        description: "Go Gin REST API",
        commands: &["go-gin"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_go_gin_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("go.mod.hbs").exists());
        assert!(dir.path().join("cmd/server/main.go.hbs").exists());
        assert!(dir.path().join("internal/handler/item.go.hbs").exists());
        assert!(dir.path().join("internal/model/item.go.hbs").exists());
    }

    #[test]
    fn test_go_gin_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-gin-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-gin-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 9);
        assert!(temp.path().join("output/go.mod").exists());
        assert!(temp.path().join("output/cmd/server/main.go").exists());
        assert!(temp.path().join("output/internal/handler/item.go").exists());
    }

    #[test]
    fn test_go_gin_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "go-gin");
    }
}
