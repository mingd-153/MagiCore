use std::collections::HashMap;
use std::path::Path;

use walkdir::WalkDir;

use crate::error::ScaffoldError;
use crate::renderer::TemplateRenderer;
use crate::engine::{OverwritePolicy, ProjectCreated};

pub struct FileGenerator {
    renderer: TemplateRenderer,
}

impl FileGenerator {
    pub fn new(renderer: TemplateRenderer) -> Self {
        Self { renderer }
    }

    pub fn generate(
        &self,
        template_dir: &Path,
        dest: &Path,
        vars: &HashMap<String, String>,
        policy: &OverwritePolicy,
    ) -> Result<ProjectCreated, ScaffoldError> {
        if !template_dir.exists() {
            return Err(ScaffoldError::TemplateNotFound(template_dir.to_path_buf()));
        }

        let mut files_created = Vec::new();

        for entry in WalkDir::new(template_dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 0 {
                    return true;
                }
                let name = e.file_name().to_str().unwrap_or("");
                if name.starts_with('.') && e.file_type().is_dir() {
                    return false;
                }
                true
            })
        {
            let entry = entry.map_err(|e| {
                let msg = e.to_string();
                ScaffoldError::Io(
                    e.into_io_error()
                        .unwrap_or_else(|| std::io::Error::other(msg)),
                )
            })?;
            let path = entry.path();

            if path == template_dir {
                continue;
            }

            let relative = path.strip_prefix(template_dir).unwrap();
            let dest_path = dest.join(relative);

            if entry.file_type().is_dir() {
                self.create_dir(&dest_path, policy)?;
                continue;
            }

            let is_hbs = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "hbs")
                .unwrap_or(false);

            let output_path = if is_hbs {
                dest_path.with_extension("")
            } else {
                dest_path
            };

            if output_path.exists() {
                match policy {
                    OverwritePolicy::Error => {
                        return Err(ScaffoldError::PathExists(output_path));
                    }
                    OverwritePolicy::Force => {
                        std::fs::remove_file(&output_path)?;
                    }
                    OverwritePolicy::Backup => {
                        let backup = output_path.with_extension("bak");
                        std::fs::rename(&output_path, &backup)?;
                    }
                }
            }

            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if is_hbs {
                let rendered = self.renderer.render_file(path, vars)?;
                std::fs::write(&output_path, rendered)?;
            } else {
                std::fs::copy(path, &output_path)?;
            }

            files_created.push(output_path);
        }

        Ok(ProjectCreated {
            name: dest
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string(),
            path: dest.to_path_buf(),
            files_created,
            features: Vec::new(),
        })
    }

    fn create_dir(&self, path: &Path, policy: &OverwritePolicy) -> Result<(), ScaffoldError> {
        if path.exists() {
            return match policy {
                OverwritePolicy::Error => Err(ScaffoldError::PathExists(path.to_path_buf())),
                OverwritePolicy::Force | OverwritePolicy::Backup => Ok(()),
            };
        }
        std::fs::create_dir_all(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::renderer::TemplateRenderer;

    #[test]
    fn test_generator_template_not_found() {
        let gen = FileGenerator::new(TemplateRenderer::new());
        let result = gen.generate(
            Path::new("/nonexistent/template"),
            Path::new("/tmp/out"),
            &HashMap::new(),
            &OverwritePolicy::Error,
        );
        assert!(matches!(result, Err(ScaffoldError::TemplateNotFound(_))));
    }

    #[test]
    fn test_generator_with_real_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let template_dir = dir.path().join("template");
        std::fs::create_dir(&template_dir).unwrap();
        std::fs::write(template_dir.join("test.txt"), "hello").unwrap();
        std::fs::write(
            template_dir.join("index.html.hbs"),
            "<h1>{{name}}</h1>",
        )
        .unwrap();

        let dest = dir.path().join("output");
        let gen = FileGenerator::new(TemplateRenderer::new());
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());

        let result = gen.generate(&template_dir, &dest, &vars, &OverwritePolicy::Error);
        assert!(result.is_ok());

        let created = result.unwrap();
        assert_eq!(created.files_created.len(), 2);

        let txt_content = std::fs::read_to_string(dest.join("test.txt")).unwrap();
        assert_eq!(txt_content, "hello");

        let html_content = std::fs::read_to_string(dest.join("index.html")).unwrap();
        assert_eq!(html_content, "<h1>World</h1>");
    }

    #[test]
    fn test_generator_overwrite_error() {
        let dir = tempfile::tempdir().unwrap();
        let template_dir = dir.path().join("template");
        std::fs::create_dir(&template_dir).unwrap();
        std::fs::write(template_dir.join("file.txt"), "content").unwrap();

        let dest = dir.path().join("output");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("file.txt"), "existing").unwrap();

        let gen = FileGenerator::new(TemplateRenderer::new());
        let result = gen.generate(&template_dir, &dest, &HashMap::new(), &OverwritePolicy::Error);
        assert!(matches!(result, Err(ScaffoldError::PathExists(_))));
    }

    #[test]
    fn test_generator_overwrite_force() {
        let dir = tempfile::tempdir().unwrap();
        let template_dir = dir.path().join("template");
        std::fs::create_dir(&template_dir).unwrap();
        std::fs::write(template_dir.join("file.txt"), "new content").unwrap();

        let dest = dir.path().join("output");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("file.txt"), "old content").unwrap();

        let gen = FileGenerator::new(TemplateRenderer::new());
        let result = gen.generate(&template_dir, &dest, &HashMap::new(), &OverwritePolicy::Force);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(dest.join("file.txt")).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_generator_with_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        let template_dir = dir.path().join("template");
        std::fs::create_dir_all(template_dir.join("src")).unwrap();
        std::fs::write(template_dir.join("src/main.js"), "console.log('hi')").unwrap();
        std::fs::write(template_dir.join("README.md"), "# {{name}}").unwrap();

        let dest = dir.path().join("project");
        let gen = FileGenerator::new(TemplateRenderer::new());
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "MyApp".to_string());

        let result = gen.generate(&template_dir, &dest, &vars, &OverwritePolicy::Error);
        assert!(result.is_ok());

        assert!(dest.join("src/main.js").exists());
        assert!(dest.join("README.md").exists());

        let readme = std::fs::read_to_string(dest.join("README.md")).unwrap();
        assert_eq!(readme, "# {{name}}");
    }
}
