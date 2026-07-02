use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::engine::{FileGenerator, OverwritePolicy, ProjectCreated, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::renderer::TemplateRenderer;
use crate::validate::NameValidator;

pub struct StaticScaffolder {
    template_dir: PathBuf,
    generator: FileGenerator,
}

impl StaticScaffolder {
    pub fn new(template_dir: PathBuf) -> Self {
        Self {
            template_dir,
            generator: FileGenerator::new(TemplateRenderer::new()),
        }
    }

    pub fn set_renderer(&mut self, renderer: TemplateRenderer) {
        self.generator = FileGenerator::new(renderer);
    }
}

impl ScaffoldEngine for StaticScaffolder {
    fn name(&self) -> &str {
        "static"
    }

    fn create_project(
        &self,
        name: &str,
        dest: &Path,
        vars: &HashMap<String, String>,
        force: bool,
    ) -> Result<ProjectCreated, ScaffoldError> {
        NameValidator::validate(name)
            .map_err(|e| ScaffoldError::InvalidName(name.to_string(), e.to_string()))?;

        let dest_path = if dest.is_absolute() {
            dest.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(ScaffoldError::Io)?
                .join(dest)
        };

        let policy = if force {
            OverwritePolicy::Force
        } else {
            OverwritePolicy::Error
        };

        let result = self.generator.generate(&self.template_dir, &dest_path, vars, &policy)?;

        Ok(ProjectCreated {
            features: result.features,
            files_created: result.files_created,
            name: name.to_string(),
            path: dest_path,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_invalid_name() {
        let dir = tempfile::tempdir().unwrap();
        let scaffolder = StaticScaffolder::new(dir.path().to_path_buf());
        let result = scaffolder.create_project(
            "",
            &dir.path().join("out"),
            &HashMap::new(),
            false,
        );
        assert!(matches!(result, Err(ScaffoldError::InvalidName(_, _))));
    }

    #[test]
    fn test_template_not_found() {
        let scaffolder = StaticScaffolder::new(PathBuf::from("/nonexistent"));
        let dir = tempfile::tempdir().unwrap();
        let result = scaffolder.create_project(
            "valid-name",
            &dir.path().join("out"),
            &HashMap::new(),
            false,
        );
        assert!(matches!(result, Err(ScaffoldError::TemplateNotFound(_))));
    }
}
