use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("requirements.txt.hbs", include_str!("django/requirements.txt.hbs")),
    ("manage.py.hbs", include_str!("django/manage.py.hbs")),
    ("config/__init__.py.hbs", include_str!("django/config/__init__.py.hbs")),
    ("config/settings.py.hbs", include_str!("django/config/settings.py.hbs")),
    ("config/urls.py.hbs", include_str!("django/config/urls.py.hbs")),
    ("config/api_urls.py.hbs", include_str!("django/config/api_urls.py.hbs")),
    ("config/wsgi.py.hbs", include_str!("django/config/wsgi.py.hbs")),
    ("apps/__init__.py.hbs", include_str!("django/apps/__init__.py.hbs")),
    ("apps/items/__init__.py.hbs", include_str!("django/apps/items/__init__.py.hbs")),
    ("apps/items/models.py.hbs", include_str!("django/apps/items/models.py.hbs")),
    ("apps/items/serializers.py.hbs", include_str!("django/apps/items/serializers.py.hbs")),
    ("apps/items/views.py.hbs", include_str!("django/apps/items/views.py.hbs")),
    ("apps/items/urls.py.hbs", include_str!("django/apps/items/urls.py.hbs")),
    ("Dockerfile.hbs", include_str!("django/Dockerfile.hbs")),
    (".gitignore.hbs", include_str!("django/.gitignore.hbs")),
    (".env.example.hbs", include_str!("django/.env.example.hbs")),
    ("README.md.hbs", include_str!("django/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct DjangoScaffolder(StaticScaffolder);

impl ScaffoldEngine for DjangoScaffolder {
    fn name(&self) -> &str {
        "django"
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
    let base = std::env::temp_dir().join("mg-django");
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
    Box::new(DjangoScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "django",
        description: "Django REST API with Python 3.12",
        commands: &["django"],
    supported_flags: &[],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_django_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("requirements.txt.hbs").exists());
        assert!(dir.path().join("manage.py.hbs").exists());
        assert!(dir.path().join("config/settings.py.hbs").exists());
        assert!(dir.path().join("apps/items/models.py.hbs").exists());
        assert!(dir.path().join("apps/items/views.py.hbs").exists());
    }

    #[test]
    fn test_django_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-django-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-django-api", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 15);
        assert!(temp.path().join("output/requirements.txt").exists());
        assert!(temp.path().join("output/manage.py").exists());
        assert!(temp.path().join("output/config/settings.py").exists());
        assert!(temp.path().join("output/apps/items/views.py").exists());
    }

    #[test]
    fn test_django_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "django");
    }
}
