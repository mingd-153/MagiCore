//! Game engine detection for mgc-game-adapter.
//! Tách nhận diện engine khỏi adapter để folder dễ mở rộng.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEngine {
    Bevy,
    Godot,
    Unity,
    Unreal,
}

impl GameEngine {
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s {
            "bevy" => Some(Self::Bevy),
            "godot" => Some(Self::Godot),
            "unity" => Some(Self::Unity),
            "unreal" => Some(Self::Unreal),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Bevy => "bevy",
            Self::Godot => "godot",
            Self::Unity => "unity",
            Self::Unreal => "unreal",
        }
    }
}

pub fn detect_engine(root: &Path) -> Option<GameEngine> {
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml"))
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem")
            .and_then(|e| e.as_str())
            .is_some_and(|eco| eco != "game")
            && v.get("game").is_none()
        {
            return None;
        }
        if let Some(engine) = v
            .get("game")
            .and_then(|g| g.get("engine"))
            .and_then(|e| e.as_str())
        {
            return GameEngine::from_str(engine);
        }
    }
    if root.join("project.godot").exists() {
        return Some(GameEngine::Godot);
    }
    if root.join("Packages").join("manifest.json").exists() {
        return Some(GameEngine::Unity);
    }
    if root.read_dir().ok()?.filter_map(|e| e.ok()).any(|e| {
        e.path()
            .extension()
            .is_some_and(|extension| extension == "uproject")
    }) {
        return Some(GameEngine::Unreal);
    }
    if root.join("Cargo.toml").exists() {
        return Some(GameEngine::Bevy);
    }
    None
}

pub(crate) fn manifest_is_game(root: &Path) -> bool {
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml"))
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem").and_then(|e| e.as_str()) == Some("game") {
            return true;
        }
        if v.get("game").is_some() {
            return true;
        }
    }
    root.join("project.godot").exists()
        || root.join("Packages").join("manifest.json").exists()
        || root.join("Cargo.toml").exists()
        || root.read_dir().ok().is_some_and(|rd| {
            rd.filter_map(|e| e.ok()).any(|e| {
                e.path()
                    .extension()
                    .is_some_and(|extension| extension == "uproject")
            })
        })
}
