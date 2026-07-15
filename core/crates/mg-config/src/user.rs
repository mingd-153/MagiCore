/// User-level configuration
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserConfig {
    pub name: Option<String>,
    pub email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::UserConfig;

    #[test]
    fn default_values() {
        let cfg = UserConfig::default();
        assert_eq!(cfg.name, None);
        assert_eq!(cfg.email, None);
    }
}
