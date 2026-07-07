pub mod generator;
pub mod r#static;

use std::collections::HashMap;
use std::path::PathBuf;

pub use generator::FileGenerator;
pub use r#static::StaticScaffolder;

#[derive(Debug)]
pub struct ProjectCreated {
    pub name: String,
    pub path: PathBuf,
    pub files_created: Vec<PathBuf>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScaffoldContext {
    pub project_name: String,
    pub project_path: PathBuf,
    pub framework: Option<String>,
    pub vars: HashMap<String, String>,
    pub features: Vec<String>,
}

impl ScaffoldContext {
    pub fn new(name: &str, path: PathBuf) -> Self {
        Self {
            project_name: name.to_string(),
            project_path: path,
            framework: None,
            vars: HashMap::new(),
            features: Vec::new(),
        }
    }

    pub fn with_framework(mut self, framework: &str) -> Self {
        self.framework = Some(framework.to_string());
        self
    }

    pub fn with_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.vars = vars;
        self
    }

    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = features;
        self
    }

    pub fn get_var(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|s| s.as_str())
    }
}

pub trait ScaffoldEngine: Send + Sync {
    fn name(&self) -> &str;
    fn create_project(
        &self,
        ctx: &ScaffoldContext,
        force: bool,
    ) -> Result<ProjectCreated, crate::error::ScaffoldError>;
}

pub enum OverwritePolicy {
    Error,
    Force,
    Backup,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_context_new() {
        let ctx = ScaffoldContext::new("test-app", PathBuf::from("/tmp/test"));
        assert_eq!(ctx.project_name, "test-app");
        assert_eq!(ctx.project_path, PathBuf::from("/tmp/test"));
        assert!(ctx.vars.is_empty());
        assert!(ctx.features.is_empty());
    }

    #[test]
    fn test_context_with_vars() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "value".to_string());
        let ctx = ScaffoldContext::new("test", PathBuf::from("/tmp/t")).with_vars(vars);

        assert_eq!(ctx.get_var("name"), Some("value"));
    }

    #[test]
    fn test_context_with_features() {
        let features = vec!["typescript".to_string(), "tailwindcss".to_string()];
        let ctx = ScaffoldContext::new("test", PathBuf::from("/tmp/t")).with_features(features);

        assert_eq!(ctx.features.len(), 2);
    }

    #[test]
    fn test_context_get_var_missing() {
        let ctx = ScaffoldContext::new("test", PathBuf::from("/tmp/t"));
        assert_eq!(ctx.get_var("nonexistent"), None);
    }

    #[test]
    fn test_context_with_framework() {
        let ctx = ScaffoldContext::new("app", PathBuf::from("/tmp/a")).with_framework("react");
        assert_eq!(ctx.framework.as_deref(), Some("react"));
    }
}
