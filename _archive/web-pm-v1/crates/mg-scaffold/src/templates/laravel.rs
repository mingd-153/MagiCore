use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("composer.json.hbs", include_str!("laravel/composer.json.hbs")),
    (".env.hbs", include_str!("laravel/.env.hbs")),
    ("config/app.php.hbs", include_str!("laravel/config/app.php.hbs")),
    (
        "config/database.php.hbs",
        include_str!("laravel/config/database.php.hbs"),
    ),
    (
        "routes/api.php.hbs",
        include_str!("laravel/routes/api.php.hbs"),
    ),
    (
        "routes/web.php.hbs",
        include_str!("laravel/routes/web.php.hbs"),
    ),
    (
        "app/Http/Controllers/Controller.php.hbs",
        include_str!("laravel/app/Http/Controllers/Controller.php.hbs"),
    ),
    (
        "app/Http/Controllers/ItemController.php.hbs",
        include_str!("laravel/app/Http/Controllers/ItemController.php.hbs"),
    ),
    (
        "app/Models/Item.php.hbs",
        include_str!("laravel/app/Models/Item.php.hbs"),
    ),
    (
        "app/Providers/RouteServiceProvider.php.hbs",
        include_str!("laravel/app/Providers/RouteServiceProvider.php.hbs"),
    ),
    (
        "database/migrations/2024_01_01_000001_create_items_table.php.hbs",
        include_str!("laravel/database/migrations/2024_01_01_000001_create_items_table.php.hbs"),
    ),
    ("artisan.hbs", include_str!("laravel/artisan.hbs")),
    (
        "public/index.php.hbs",
        include_str!("laravel/public/index.php.hbs"),
    ),
    ("Dockerfile.hbs", include_str!("laravel/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("laravel/.gitignore.hbs")),
    ("README.md.hbs", include_str!("laravel/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct LaravelScaffolder(StaticScaffolder);

impl ScaffoldEngine for LaravelScaffolder {
    fn name(&self) -> &str {
        "laravel"
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
    let base = std::env::temp_dir().join("mg-laravel");
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
    Box::new(LaravelScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "laravel",
        description: "Laravel REST API with PHP 8.3",
        commands: &["laravel"],
    supported_flags: &[],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_laravel_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("composer.json.hbs").exists());
        assert!(dir.path().join("routes/api.php.hbs").exists());
        assert!(dir.path().join("Dockerfile.hbs").exists());
    }

    #[test]
    fn test_laravel_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-laravel-api".to_string());

        let ctx =
            ScaffoldContext::new("my-laravel-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 10);
        assert!(temp.path().join("output/composer.json").exists());
        assert!(temp.path().join("output/config/app.php").exists());
        assert!(temp.path().join("output/routes/api.php").exists());
    }

    #[test]
    fn test_laravel_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "laravel");
    }
}
