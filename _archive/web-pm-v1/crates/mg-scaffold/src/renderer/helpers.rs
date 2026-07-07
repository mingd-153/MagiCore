use handlebars::Handlebars;
use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};

pub fn register_all(registry: &mut Handlebars<'static>) {
    registry.register_helper(
        "pascalCase",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
                out.write(&param.to_upper_camel_case())?;
                Ok(())
            },
        ),
    );

    registry.register_helper(
        "camelCase",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
                out.write(&param.to_lower_camel_case())?;
                Ok(())
            },
        ),
    );

    registry.register_helper(
        "kebabCase",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
                out.write(&param.to_kebab_case())?;
                Ok(())
            },
        ),
    );

    registry.register_helper(
        "snakeCase",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
                out.write(&param.to_snake_case())?;
                Ok(())
            },
        ),
    );

    registry.register_helper(
        "upperCase",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
                out.write(&param.to_uppercase())?;
                Ok(())
            },
        ),
    );
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashMap;

    fn render_with_helper(template: &str, var_name: &str, var_value: &str) -> String {
        let mut registry = Handlebars::new();
        registry.register_escape_fn(handlebars::no_escape);
        register_all(&mut registry);

        let mut vars = HashMap::new();
        vars.insert(var_name.to_string(), var_value.to_string());

        registry.render_template(template, &vars).unwrap()
    }

    #[test]
    fn test_pascal_case() {
        let result = render_with_helper("{{pascalCase name}}", "name", "my-app");
        assert_eq!(result, "MyApp");
    }

    #[test]
    fn test_camel_case() {
        let result = render_with_helper("{{camelCase name}}", "name", "my-app");
        assert_eq!(result, "myApp");
    }

    #[test]
    fn test_kebab_case() {
        let result = render_with_helper("{{kebabCase name}}", "name", "MyApp");
        assert_eq!(result, "my-app");
    }

    #[test]
    fn test_snake_case() {
        let result = render_with_helper("{{snakeCase name}}", "name", "my-app");
        assert_eq!(result, "my_app");
    }

    #[test]
    fn test_upper_case() {
        let result = render_with_helper("{{upperCase name}}", "name", "my-app");
        assert_eq!(result, "MY-APP");
    }

    #[test]
    fn test_edge_cases() {
        assert_eq!(render_with_helper("{{pascalCase name}}", "name", ""), "");
        assert_eq!(render_with_helper("{{camelCase name}}", "name", "a"), "a");
        assert_eq!(render_with_helper("{{snakeCase name}}", "name", "A"), "a");
    }
}
