use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("requirements.txt.hbs", include_str!("flask/requirements.txt.hbs")),
    ("app/__init__.py.hbs", include_str!("flask/app/__init__.py.hbs")),
    ("app/config.py.hbs", include_str!("flask/app/config.py.hbs")),
    ("app/models/__init__.py.hbs", include_str!("flask/app/models/__init__.py.hbs")),
    ("app/models/item.py.hbs", include_str!("flask/app/models/item.py.hbs")),
    ("app/schemas/__init__.py.hbs", include_str!("flask/app/schemas/__init__.py.hbs")),
    ("app/schemas/item.py.hbs", include_str!("flask/app/schemas/item.py.hbs")),
    ("app/routers/__init__.py.hbs", include_str!("flask/app/routers/__init__.py.hbs")),
    ("app/routers/items.py.hbs", include_str!("flask/app/routers/items.py.hbs")),
    ("Dockerfile.hbs", include_str!("flask/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("flask/.gitignore.hbs")),
    (".env.example.hbs", include_str!("flask/.env.example.hbs")),
    ("README.md.hbs", include_str!("flask/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct FlaskScaffolder(StaticScaffolder);

impl ScaffoldEngine for FlaskScaffolder {
    fn name(&self) -> &str {
        "flask"
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
    let base = std::env::temp_dir().join("mgpm-flask");
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
    Box::new(FlaskScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "flask",
        description: "Flask REST API with Python 3.12",
        commands: &["flask"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_flask_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("requirements.txt.hbs").exists());
        assert!(dir.path().join("app/__init__.py.hbs").exists());
        assert!(dir.path().join("app/models/item.py.hbs").exists());
        assert!(dir.path().join("app/routers/items.py.hbs").exists());
    }

    #[test]
    fn test_flask_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-flask-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-flask-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 11);
        assert!(temp.path().join("output/requirements.txt").exists());
        assert!(temp.path().join("output/app/__init__.py").exists());
        assert!(temp.path().join("output/app/routers/items.py").exists());
    }

    #[test]
    fn test_flask_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "flask");
    }
}
