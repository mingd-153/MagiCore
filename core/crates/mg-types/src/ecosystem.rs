#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Ecosystem {
    Web,
    Game,
    Ai,
    Cloud,
    Cicd,
    Iot,
    App,
    Lib,
}

impl Ecosystem {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "web" => Some(Self::Web),
            "game" => Some(Self::Game),
            "ai" => Some(Self::Ai),
            "cloud" | "clo" => Some(Self::Cloud),
            "cicd" => Some(Self::Cicd),
            "iot" => Some(Self::Iot),
            "app" => Some(Self::App),
            "lib" => Some(Self::Lib),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Game => "game",
            Self::Ai => "ai",
            Self::Cloud => "cloud",
            Self::Cicd => "cicd",
            Self::Iot => "iot",
            Self::App => "app",
            Self::Lib => "lib",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ecosystem;

    #[test]
    fn from_str_all_variants() {
        assert_eq!(Ecosystem::from_str("web"), Some(Ecosystem::Web));
        assert_eq!(Ecosystem::from_str("game"), Some(Ecosystem::Game));
        assert_eq!(Ecosystem::from_str("ai"), Some(Ecosystem::Ai));
        assert_eq!(Ecosystem::from_str("cloud"), Some(Ecosystem::Cloud));
        assert_eq!(Ecosystem::from_str("cicd"), Some(Ecosystem::Cicd));
        assert_eq!(Ecosystem::from_str("iot"), Some(Ecosystem::Iot));
        assert_eq!(Ecosystem::from_str("app"), Some(Ecosystem::App));
        assert_eq!(Ecosystem::from_str("lib"), Some(Ecosystem::Lib));
    }

    #[test]
    fn from_str_cloud_alias() {
        assert_eq!(Ecosystem::from_str("clo"), Some(Ecosystem::Cloud));
    }

    #[test]
    fn from_str_unknown_returns_none() {
        assert_eq!(Ecosystem::from_str("unknown"), None);
        assert_eq!(Ecosystem::from_str(""), None);
    }

    #[test]
    fn as_str_all_variants() {
        assert_eq!(Ecosystem::Web.as_str(), "web");
        assert_eq!(Ecosystem::Game.as_str(), "game");
        assert_eq!(Ecosystem::Ai.as_str(), "ai");
        assert_eq!(Ecosystem::Cloud.as_str(), "cloud");
        assert_eq!(Ecosystem::Cicd.as_str(), "cicd");
        assert_eq!(Ecosystem::Iot.as_str(), "iot");
        assert_eq!(Ecosystem::App.as_str(), "app");
        assert_eq!(Ecosystem::Lib.as_str(), "lib");
    }

    #[test]
    fn roundtrip_all_variants() {
        for variant in [
            Ecosystem::Web,
            Ecosystem::Game,
            Ecosystem::Ai,
            Ecosystem::Cloud,
            Ecosystem::Cicd,
            Ecosystem::Iot,
            Ecosystem::App,
            Ecosystem::Lib,
        ] {
            let s = variant.as_str();
            assert_eq!(Ecosystem::from_str(s), Some(variant));
        }
    }

    #[test]
    fn debug_format() {
        let s = format!("{:?}", Ecosystem::Web);
        assert_eq!(s, "Web");
    }

    #[test]
    fn copy_works() {
        let a = Ecosystem::Ai;
        let b = a;
        assert_eq!(a, b);
    }
}
