use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, write_favicon, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("package.json.hbs", include_str!("next/package.json.hbs")),
    (
        "next.config.ts.hbs",
        include_str!("next/next.config.ts.hbs"),
    ),
    (
        "tsconfig.json.hbs",
        include_str!("next/tsconfig.json.hbs"),
    ),
    (
        "tailwind.config.ts.hbs",
        include_str!("next/tailwind.config.ts.hbs"),
    ),
    (
        "postcss.config.mjs.hbs",
        include_str!("next/postcss.config.mjs.hbs"),
    ),
    (
        "eslint.config.mjs.hbs",
        include_str!("next/eslint.config.mjs.hbs"),
    ),
    (
        "src/app/layout.tsx.hbs",
        include_str!("next/src/app/layout.tsx.hbs"),
    ),
    (
        "src/app/page.tsx.hbs",
        include_str!("next/src/app/page.tsx.hbs"),
    ),
    (
        "src/app/not-found.tsx.hbs",
        include_str!("next/src/app/not-found.tsx.hbs"),
    ),
    (
        "src/app/globals.css.hbs",
        include_str!("next/src/app/globals.css.hbs"),
    ),
    (
        "src/app/(marketing)/about/page.tsx.hbs",
        include_str!("next/src/app/(marketing)/about/page.tsx.hbs"),
    ),
    (
        "src/app/(dashboard)/layout.tsx.hbs",
        include_str!("next/src/app/(dashboard)/layout.tsx.hbs"),
    ),
    (
        "src/app/(dashboard)/page.tsx.hbs",
        include_str!("next/src/app/(dashboard)/page.tsx.hbs"),
    ),
    (
        "src/app/api/hello/route.ts.hbs",
        include_str!("next/src/app/api/hello/route.ts.hbs"),
    ),
    (
        "src/app/api/auth/route.ts.hbs",
        include_str!("next/src/app/api/auth/route.ts.hbs"),
    ),
    (
        "src/actions/auth.ts.hbs",
        include_str!("next/src/actions/auth.ts.hbs"),
    ),
    (
        "src/lib/utils.ts.hbs",
        include_str!("next/src/lib/utils.ts.hbs"),
    ),
    (
        "src/lib/db.ts.hbs",
        include_str!("next/src/lib/db.ts.hbs"),
    ),
    (
        "src/components/ui/button.tsx.hbs",
        include_str!("next/src/components/ui/button.tsx.hbs"),
    ),
    (
        "src/components/ui/input.tsx.hbs",
        include_str!("next/src/components/ui/input.tsx.hbs"),
    ),
    (
        "src/components/ui/card.tsx.hbs",
        include_str!("next/src/components/ui/card.tsx.hbs"),
    ),
    (
        "src/components/features/header.tsx.hbs",
        include_str!("next/src/components/features/header.tsx.hbs"),
    ),
    (
        "src/components/features/footer.tsx.hbs",
        include_str!("next/src/components/features/footer.tsx.hbs"),
    ),
    ("Dockerfile.hbs", include_str!("next/Dockerfile.hbs")),
    (
        ".github/workflows/ci.yml.hbs",
        include_str!("next/.github/workflows/ci.yml.hbs"),
    ),
    (".env.example.hbs", include_str!("next/.env.example.hbs")),
    (
        ".env.local.example.hbs",
        include_str!("next/.env.local.example.hbs"),
    ),
    (".gitignore.hbs", include_str!("next/.gitignore.hbs")),
    ("README.md.hbs", include_str!("next/README.md.hbs")),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct NextScaffolder(StaticScaffolder);

impl ScaffoldEngine for NextScaffolder {
    fn name(&self) -> &str {
        "next"
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
    let base = std::env::temp_dir().join("mg-next");
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
    Box::new(NextScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "next",
        description: "Next.js fullstack app with App Router",
        commands: &["next"],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_next_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("package.json.hbs").exists());
        assert!(dir.path().join("next.config.ts.hbs").exists());
        assert!(dir.path().join("src/app/layout.tsx.hbs").exists());
        assert!(dir.path().join("src/app/page.tsx.hbs").exists());
        assert!(dir.path().join("src/actions/auth.ts.hbs").exists());
        assert!(dir.path().join("Dockerfile.hbs").exists());
        assert!(dir.path().join(".github/workflows/ci.yml.hbs").exists());
    }

    #[test]
    fn test_next_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-next-app".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx =
            ScaffoldContext::new("my-next-app", temp.path().join("output")).with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 25);
        assert!(temp.path().join("output/package.json").exists());
        assert!(temp.path().join("output/src/app/layout.tsx").exists());
        assert!(temp.path().join("output/src/actions/auth.ts").exists());
        assert!(temp.path().join("output/Dockerfile").exists());
    }

    #[test]
    fn test_next_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "next");
    }
}
