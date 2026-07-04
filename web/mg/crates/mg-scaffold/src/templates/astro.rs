use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, write_favicon, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    (
        "package.json.hbs",
        include_str!("astro/package.json.hbs"),
    ),
    (
        "tsconfig.json.hbs",
        include_str!("astro/tsconfig.json.hbs"),
    ),
    (
        "astro.config.mjs.hbs",
        include_str!("astro/astro.config.mjs.hbs"),
    ),
    (
        "src/env.d.ts",
        include_str!("astro/src/env.d.ts"),
    ),
    (
        "src/pages/index.astro.hbs",
        include_str!("astro/src/pages/index.astro.hbs"),
    ),
    (
        "src/pages/about.astro.hbs",
        include_str!("astro/src/pages/about.astro.hbs"),
    ),
    (
        "src/layouts/Layout.astro.hbs",
        include_str!("astro/src/layouts/Layout.astro.hbs"),
    ),
    (
        "src/components/Header.astro.hbs",
        include_str!("astro/src/components/Header.astro.hbs"),
    ),
    (
        "src/styles/global.css.hbs",
        include_str!("astro/src/styles/global.css.hbs"),
    ),
    (".gitignore.hbs", include_str!("astro/.gitignore.hbs")),
    (".env.example.hbs", include_str!("astro/.env.example.hbs")),
    ("README.md.hbs", include_str!("astro/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct AstroScaffolder(StaticScaffolder);

impl ScaffoldEngine for AstroScaffolder {
    fn name(&self) -> &str {
        "astro"
    }

    fn create_project(
        &self,
        ctx: &ScaffoldContext,
        force: bool,
    ) -> Result<ProjectCreated, ScaffoldError> {
        let result = self.0.create_project(ctx, force)?;
        write_favicon(&result.path)?;
        Ok(result)
    }
}

fn create_temp_dir() -> Result<PathBuf, ScaffoldError> {
    let base = std::env::temp_dir().join("mg-astro");
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
    Box::new(AstroScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "astro",
        description: "Astro static site with TypeScript",
        commands: &["astro"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_astro_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("astro.config.mjs.hbs").exists());
        assert!(dir.path().join("src/pages/index.astro.hbs").exists());
        assert!(dir.path().join("src/layouts/Layout.astro.hbs").exists());
    }

    #[test]
    fn test_astro_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-astro-site".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-astro-site", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 10);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/pages/index.astro").exists());
        assert!(temp.path().join("output/astro.config.mjs").exists());
    }

    #[test]
    fn test_astro_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "astro");
    }
}
