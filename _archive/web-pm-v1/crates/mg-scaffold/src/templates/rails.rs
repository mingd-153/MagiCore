use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("Gemfile.hbs", include_str!("rails/Gemfile.hbs")),
    (
        "config/application.rb.hbs",
        include_str!("rails/config/application.rb.hbs"),
    ),
    ("config/boot.rb.hbs", include_str!("rails/config/boot.rb.hbs")),
    (
        "config/database.yml.hbs",
        include_str!("rails/config/database.yml.hbs"),
    ),
    (
        "config/routes.rb.hbs",
        include_str!("rails/config/routes.rb.hbs"),
    ),
    (
        "app/controllers/application_controller.rb.hbs",
        include_str!("rails/app/controllers/application_controller.rb.hbs"),
    ),
    (
        "app/controllers/health_controller.rb.hbs",
        include_str!("rails/app/controllers/health_controller.rb.hbs"),
    ),
    (
        "app/controllers/items_controller.rb.hbs",
        include_str!("rails/app/controllers/items_controller.rb.hbs"),
    ),
    (
        "app/models/application_record.rb.hbs",
        include_str!("rails/app/models/application_record.rb.hbs"),
    ),
    (
        "app/models/item.rb.hbs",
        include_str!("rails/app/models/item.rb.hbs"),
    ),
    (
        "db/migrate/20240101000001_create_items.rb.hbs",
        include_str!("rails/db/migrate/20240101000001_create_items.rb.hbs"),
    ),
    ("config.ru.hbs", include_str!("rails/config.ru.hbs")),
    ("Rakefile.hbs", include_str!("rails/Rakefile.hbs")),
    ("Dockerfile.hbs", include_str!("rails/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("rails/.gitignore.hbs")),
    (
        ".env.example.hbs",
        include_str!("rails/.env.example.hbs"),
    ),
    ("README.md.hbs", include_str!("rails/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct RailsScaffolder(StaticScaffolder);

impl ScaffoldEngine for RailsScaffolder {
    fn name(&self) -> &str {
        "rails"
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
    let base = std::env::temp_dir().join("mg-rails");
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
    Box::new(RailsScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "rails",
        description: "Ruby on Rails API with Ruby 3.3",
        commands: &["rails"],
    supported_flags: &[],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_rails_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("Gemfile.hbs").exists());
        assert!(dir.path().join("config/application.rb.hbs").exists());
        assert!(dir.path().join("config/database.yml.hbs").exists());
        assert!(dir.path().join("config/routes.rb.hbs").exists());
        assert!(dir.path().join("app/controllers/items_controller.rb.hbs").exists());
        assert!(dir.path().join("app/models/item.rb.hbs").exists());
        assert!(dir.path().join("db/migrate/20240101000001_create_items.rb.hbs").exists());
        assert!(dir.path().join("Dockerfile.hbs").exists());
        assert!(dir.path().join(".gitignore.hbs").exists());
        assert!(dir.path().join(".env.example.hbs").exists());
        assert!(dir.path().join("README.md.hbs").exists());
    }

    #[test]
    fn test_rails_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-rails-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-rails-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 16);
        assert!(temp.path().join("output/Gemfile").exists());
        assert!(temp.path().join("output/config/application.rb").exists());
        assert!(temp.path().join("output/config/database.yml").exists());
        assert!(temp.path().join("output/app/controllers/items_controller.rb").exists());
    }

    #[test]
    fn test_rails_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "rails");
    }
}
