pub mod helpers;

use std::collections::HashMap;
use std::path::Path;

use handlebars::Handlebars;

use crate::error::ScaffoldError;

pub struct TemplateRenderer {
    registry: Handlebars<'static>,
}

impl TemplateRenderer {
    pub fn new() -> Self {
        let mut registry = Handlebars::new();
        registry.register_escape_fn(handlebars::no_escape);
        helpers::register_all(&mut registry);
        Self { registry }
    }

    pub fn render(&self, template: &str, vars: &HashMap<String, String>) -> Result<String, ScaffoldError> {
        self.registry
            .render_template(template, vars)
            .map_err(|e| ScaffoldError::Template(e.to_string()))
    }

    pub fn render_file(&self, path: &Path, vars: &HashMap<String, String>) -> Result<String, ScaffoldError> {
        let content = std::fs::read_to_string(path).map_err(ScaffoldError::Io)?;
        self.render(&content, vars)
    }
}

impl Default for TemplateRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_render_simple_variable() {
        let renderer = TemplateRenderer::new();
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "my-app".to_string());

        let result = renderer.render("Hello {{name}}!", &vars).unwrap();
        assert_eq!(result, "Hello my-app!");
    }

    #[test]
    fn test_render_pascal_case_helper() {
        let renderer = TemplateRenderer::new();
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "my-app".to_string());

        let result = renderer.render("Hello {{pascalCase name}}!", &vars).unwrap();
        assert_eq!(result, "Hello MyApp!");
    }

    #[test]
    fn test_render_camel_case_helper() {
        let renderer = TemplateRenderer::new();
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "my-app".to_string());

        let result = renderer.render("Hello {{camelCase name}}!", &vars).unwrap();
        assert_eq!(result, "Hello myApp!");
    }

    #[test]
    fn test_render_multiple_vars() {
        let renderer = TemplateRenderer::new();
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "test".to_string());
        vars.insert("version".to_string(), "1.0.0".to_string());

        let result = renderer
            .render("{\"name\": \"{{name}}\", \"version\": \"{{version}}\"}", &vars)
            .unwrap();
        assert_eq!(result, "{\"name\": \"test\", \"version\": \"1.0.0\"}");
    }

    #[test]
    fn test_render_missing_var() {
        let renderer = TemplateRenderer::new();
        let vars = HashMap::new();

        let result = renderer.render("Hello {{name}}!", &vars).unwrap();
        assert_eq!(result, "Hello !");
    }

    #[test]
    fn test_render_no_template() {
        let renderer = TemplateRenderer::new();
        let vars = HashMap::new();

        let result = renderer.render("static content", &vars).unwrap();
        assert_eq!(result, "static content");
    }

    #[test]
    fn test_render_empty_template() {
        let renderer = TemplateRenderer::new();
        let vars = HashMap::new();

        let result = renderer.render("", &vars).unwrap();
        assert_eq!(result, "");
    }
}
