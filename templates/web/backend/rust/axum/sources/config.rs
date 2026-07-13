use std::env;

pub struct Config {
    pub name: String,
    pub framework: String,
    pub port: String,
}

impl Config {
    pub fn load() -> Self {
        let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
        Self {
            name: "{{project_name}}".to_string(),
            framework: "axum".to_string(),
            port,
        }
    }
}
