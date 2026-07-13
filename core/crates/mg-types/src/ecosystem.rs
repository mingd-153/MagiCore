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
