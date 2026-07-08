use std::path::PathBuf;

use crate::engine::{
    FileGenerator, OverwritePolicy, ProjectCreated, ScaffoldContext, ScaffoldEngine,
};
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
        ctx: &ScaffoldContext,
        force: bool,
    ) -> Result<ProjectCreated, ScaffoldError> {
        NameValidator::validate(&ctx.project_name)
            .map_err(|e| ScaffoldError::InvalidName(ctx.project_name.clone(), e.to_string()))?;

        let base = std::env::current_dir().map_err(|e| ScaffoldError::IoError {
            context: "current_dir".to_string(),
            source: e,
        })?;

        let dest_path = if ctx.project_path.is_absolute() {
            std::fs::canonicalize(&ctx.project_path).unwrap_or(ctx.project_path.clone())
        } else {
            let cwd = base.canonicalize().map_err(|e| ScaffoldError::IoError {
                context: "canonicalize current dir".to_string(),
                source: e,
            })?;
            cwd.join(&ctx.project_path)
        };

        let policy = if force {
            OverwritePolicy::Force
        } else {
            OverwritePolicy::Error
        };

        let mut vars = ctx.vars.clone();
        for feature in &ctx.features {
            vars.insert(feature.clone(), "true".to_string());
        }

        let result = self
            .generator
            .generate(&self.template_dir, &dest_path, &vars, &policy)?;

        Ok(ProjectCreated {
            features: ctx.features.clone(),
            files_created: result.files_created,
            name: ctx.project_name.clone(),
            path: dest_path,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn ctx(name: &str, dest: PathBuf) -> ScaffoldContext {
        ScaffoldContext::new(name, dest)
    }

    #[test]
    fn test_invalid_name() {
        let dir = tempfile::tempdir().unwrap();
        let scaffolder = StaticScaffolder::new(dir.path().to_path_buf());
        let result = scaffolder.create_project(&ctx("", dir.path().join("out")), false);
        assert!(matches!(result, Err(ScaffoldError::InvalidName(_, _))));
    }

    #[test]
    fn test_template_not_found() {
        let scaffolder = StaticScaffolder::new(PathBuf::from("/nonexistent"));
        let dir = tempfile::tempdir().unwrap();
        let result = scaffolder.create_project(&ctx("valid-name", dir.path().join("out")), false);
        assert!(matches!(result, Err(ScaffoldError::TemplateNotFound(_))));
    }
}
