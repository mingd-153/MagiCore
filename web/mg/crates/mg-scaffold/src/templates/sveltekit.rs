use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, write_favicon, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("sveltekit/package.json.hbs")),
    ("svelte.config.js.hbs", include_str!("sveltekit/svelte.config.js.hbs")),
    ("vite.config.ts.hbs", include_str!("sveltekit/vite.config.ts.hbs")),
    ("tsconfig.json.hbs", include_str!("sveltekit/tsconfig.json.hbs")),
    ("app.html.hbs", include_str!("sveltekit/app.html.hbs")),
    ("src/app.d.ts", include_str!("sveltekit/src/app.d.ts")),
    ("src/app.css.hbs", include_str!("sveltekit/src/app.css.hbs")),
    ("src/routes/+page.svelte.hbs", include_str!("sveltekit/src/routes/+page.svelte.hbs")),
    ("src/routes/about/+page.svelte.hbs", include_str!("sveltekit/src/routes/about/+page.svelte.hbs")),
    ("src/lib/components/Header.svelte.hbs", include_str!("sveltekit/src/lib/components/Header.svelte.hbs")),
    ("src/lib/utils/helpers.ts.hbs", include_str!("sveltekit/src/lib/utils/helpers.ts.hbs")),
    (".gitignore.hbs", include_str!("sveltekit/.gitignore.hbs")),
    (".env.example.hbs", include_str!("sveltekit/.env.example.hbs")),
    ("README.md.hbs", include_str!("sveltekit/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct SvelteKitScaffolder(StaticScaffolder);

impl ScaffoldEngine for SvelteKitScaffolder {
    fn name(&self) -> &str {
        "sveltekit"
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
    let base = std::env::temp_dir().join("mg-sveltekit");
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
    Box::new(SvelteKitScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "sveltekit",
        description: "SvelteKit web app with TypeScript",
        commands: &["sveltekit"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sveltekit_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("svelte.config.js.hbs").exists());
        assert!(dir.path().join("src/routes/+page.svelte.hbs").exists());
    }

    #[test]
    fn test_sveltekit_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-sveltekit-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-sveltekit-app", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 12);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/routes/+page.svelte").exists());
    }

    #[test]
    fn test_sveltekit_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "sveltekit");
    }
}
