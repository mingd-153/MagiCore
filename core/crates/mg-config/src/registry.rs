/// Registry configuration
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub name: String,
    pub url: String,
    pub priority: u32,
}

impl Registry {
    pub fn new(name: String, url: String) -> Self {
        Self {
            name,
            url,
            priority: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Registry;

    #[test]
    fn new_sets_name_url_and_default_priority() {
        let reg = Registry::new("my-reg".into(), "https://example.com".into());
        assert_eq!(reg.name, "my-reg");
        assert_eq!(reg.url, "https://example.com");
        assert_eq!(reg.priority, 0);
    }
}
