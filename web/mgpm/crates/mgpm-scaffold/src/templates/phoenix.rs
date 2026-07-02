use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("mix.exs.hbs", include_str!("phoenix/mix.exs.hbs")),
    (
        "config/config.exs.hbs",
        include_str!("phoenix/config/config.exs.hbs"),
    ),
    (
        "config/dev.exs.hbs",
        include_str!("phoenix/config/dev.exs.hbs"),
    ),
    (
        "config/runtime.exs.hbs",
        include_str!("phoenix/config/runtime.exs.hbs"),
    ),
    (
        "lib/app/application.ex.hbs",
        include_str!("phoenix/lib/app/application.ex.hbs"),
    ),
    (
        "lib/app/repo.ex.hbs",
        include_str!("phoenix/lib/app/repo.ex.hbs"),
    ),
    (
        "lib/app/accounts/item.ex.hbs",
        include_str!("phoenix/lib/app/accounts/item.ex.hbs"),
    ),
    (
        "lib/app_web/endpoint.ex.hbs",
        include_str!("phoenix/lib/app_web/endpoint.ex.hbs"),
    ),
    (
        "lib/app_web/router.ex.hbs",
        include_str!("phoenix/lib/app_web/router.ex.hbs"),
    ),
    (
        "lib/app_web/controllers/health_controller.ex.hbs",
        include_str!("phoenix/lib/app_web/controllers/health_controller.ex.hbs"),
    ),
    (
        "lib/app_web/controllers/item_controller.ex.hbs",
        include_str!("phoenix/lib/app_web/controllers/item_controller.ex.hbs"),
    ),
    (
        "lib/app/accounts.ex.hbs",
        include_str!("phoenix/lib/app/accounts.ex.hbs"),
    ),
    (
        "priv/repo/migrations/20240101000001_create_items.exs.hbs",
        include_str!("phoenix/priv/repo/migrations/20240101000001_create_items.exs.hbs"),
    ),
    ("Dockerfile.hbs", include_str!("phoenix/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("phoenix/.gitignore.hbs")),
    (
        ".env.example.hbs",
        include_str!("phoenix/.env.example.hbs"),
    ),
    ("README.md.hbs", include_str!("phoenix/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct PhoenixScaffolder(StaticScaffolder);

impl ScaffoldEngine for PhoenixScaffolder {
    fn name(&self) -> &str {
        "phoenix"
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
    let base = std::env::temp_dir().join("mgpm-phoenix");
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
    Box::new(PhoenixScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "phoenix",
        description: "Phoenix API with Elixir 1.17",
        commands: &["phoenix"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_phoenix_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("mix.exs.hbs").exists());
        assert!(dir.path().join("config/config.exs.hbs").exists());
        assert!(dir.path().join("config/dev.exs.hbs").exists());
        assert!(dir.path().join("lib/app/application.ex.hbs").exists());
        assert!(dir.path().join("lib/app/repo.ex.hbs").exists());
        assert!(dir.path().join("lib/app/accounts/item.ex.hbs").exists());
        assert!(dir.path().join("lib/app_web/router.ex.hbs").exists());
        assert!(dir.path().join("lib/app_web/controllers/item_controller.ex.hbs").exists());
        assert!(dir.path().join("lib/app/accounts.ex.hbs").exists());
        assert!(dir.path().join("priv/repo/migrations/20240101000001_create_items.exs.hbs").exists());
        assert!(dir.path().join("Dockerfile.hbs").exists());
        assert!(dir.path().join(".gitignore.hbs").exists());
        assert!(dir.path().join(".env.example.hbs").exists());
        assert!(dir.path().join("README.md.hbs").exists());
    }

    #[test]
    fn test_phoenix_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-phoenix-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-phoenix-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 15);
        assert!(temp.path().join("output/mix.exs").exists());
        assert!(temp.path().join("output/config/config.exs").exists());
        assert!(temp.path().join("output/lib/app/application.ex").exists());
        assert!(temp.path().join("output/lib/app_web/router.ex").exists());
        assert!(temp.path().join("output/priv/repo/migrations/20240101000001_create_items.exs").exists());
    }

    #[test]
    fn test_phoenix_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "phoenix");
    }
}
