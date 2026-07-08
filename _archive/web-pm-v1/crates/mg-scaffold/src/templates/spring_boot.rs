use crate::engine::ScaffoldEngine;
use crate::error::ScaffoldError;
use crate::templates::{extract_embedded, Template};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const FILES: &[(&str, &str)] = &[
    ("pom.xml.hbs", include_str!("spring-boot/pom.xml.hbs")),
    (
        "src/main/java/com/example/Application.java.hbs",
        include_str!("spring-boot/src/main/java/com/example/Application.java.hbs"),
    ),
    (
        "src/main/java/com/example/controller/ItemController.java.hbs",
        include_str!(
            "spring-boot/src/main/java/com/example/controller/ItemController.java.hbs"
        ),
    ),
    (
        "src/main/java/com/example/model/Item.java.hbs",
        include_str!("spring-boot/src/main/java/com/example/model/Item.java.hbs"),
    ),
    (
        "src/main/java/com/example/repository/ItemRepository.java.hbs",
        include_str!(
            "spring-boot/src/main/java/com/example/repository/ItemRepository.java.hbs"
        ),
    ),
    (
        "src/main/java/com/example/service/ItemService.java.hbs",
        include_str!(
            "spring-boot/src/main/java/com/example/service/ItemService.java.hbs"
        ),
    ),
    (
        "src/main/java/com/example/dto/CreateItemDTO.java.hbs",
        include_str!(
            "spring-boot/src/main/java/com/example/dto/CreateItemDTO.java.hbs"
        ),
    ),
    (
        "src/main/java/com/example/dto/ItemDTO.java.hbs",
        include_str!("spring-boot/src/main/java/com/example/dto/ItemDTO.java.hbs"),
    ),
    (
        "src/main/resources/application.properties.hbs",
        include_str!(
            "spring-boot/src/main/resources/application.properties.hbs"
        ),
    ),
    (
        "Dockerfile.hbs",
        include_str!("spring-boot/Dockerfile.hbs"),
    ),
    (
        ".gitignore.hbs",
        include_str!("spring-boot/.gitignore.hbs"),
    ),
    (
        ".env.example.hbs",
        include_str!("spring-boot/.env.example.hbs"),
    ),
    (
        "README.md.hbs",
        include_str!("spring-boot/README.md.hbs"),
    ),
];

use crate::engine::{ProjectCreated, ScaffoldContext, StaticScaffolder};

pub struct SpringBootScaffolder(StaticScaffolder);

impl ScaffoldEngine for SpringBootScaffolder {
    fn name(&self) -> &str {
        "spring-boot"
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
    let base = std::env::temp_dir().join("mg-spring-boot");
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
    Box::new(SpringBootScaffolder(StaticScaffolder::new(path)))
}

pub fn template() -> Template {
    Template {
        name: "spring-boot",
        description: "Spring Boot REST API with Java 21",
        commands: &["spring-boot"],
    supported_flags: &[],
        create_engine: build_engine,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_spring_boot_template_files() {
        let dir = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(dir.path(), &map).unwrap();

        assert!(dir.path().join("pom.xml.hbs").exists());
        assert!(dir
            .path()
            .join("src/main/java/com/example/Application.java.hbs")
            .exists());
        assert!(dir
            .path()
            .join("src/main/java/com/example/controller/ItemController.java.hbs")
            .exists());
        assert!(dir
            .path()
            .join("src/main/java/com/example/dto/CreateItemDTO.java.hbs")
            .exists());
    }

    #[test]
    fn test_spring_boot_engine_creates_project() {
        let temp = tempfile::tempdir().unwrap();
        let map: HashMap<&str, &str> = FILES.iter().copied().collect();
        extract_embedded(temp.path(), &map).unwrap();

        let scaffolder = StaticScaffolder::new(temp.path().to_path_buf());
        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), "my-spring-boot-api".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let ctx = ScaffoldContext::new(
            "my-spring-boot-api",
            temp.path().join("output"),
        )
        .with_vars(vars);

        let result = scaffolder.create_project(&ctx, false).unwrap();
        assert!(result.files_created.len() >= 10);
        assert!(temp.path().join("output/pom.xml").exists());
        assert!(temp
            .path()
            .join("output/src/main/java/com/example/Application.java")
            .exists());
        assert!(temp
            .path()
            .join("output/src/main/java/com/example/controller/ItemController.java")
            .exists());
    }

    #[test]
    fn test_spring_boot_build_engine() {
        let engine = build_engine();
        assert_eq!(engine.name(), "spring-boot");
    }
}
