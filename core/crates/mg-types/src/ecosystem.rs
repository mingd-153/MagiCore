//! Ecosystem / domain types for MegaGate adapters

use std::fmt;
use serde::{Deserialize, Serialize};

/// The ecosystem (domain) this project/adapter belongs to.
/// Used for: `mg init`, `mg create-*`, adapter auto-detection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    /// JavaScript/TypeScript (npm packages, node_modules)
    Web,
    /// Python (PyPI, venv, models)
    Ai,
    /// Game engines (Bevy/Cargo, Unity/UPM, Unreal, Godot)
    Game,
    /// Cloud infrastructure (Pulumi, Terraform, CDK)
    Cloud,
    /// CI/CD pipelines (GitHub Actions, deploy Apple/Google)
    Cicd,
    /// Mobile + Desktop apps (Flutter, Kotlin, Swift, Tauri)
    App,
    /// IoT / Embedded (PlatformIO, Zephyr, embedded-Rust)
    Iot,
    /// Generic Rust/TS/Python library
    Lib,
}

impl Ecosystem {
    /// All available ecosystems (used for `mg init` menu)
    pub fn all() -> &'static [Ecosystem] {
        &[
            Ecosystem::Web,
            Ecosystem::Ai,
            Ecosystem::Game,
            Ecosystem::Cloud,
            Ecosystem::Cicd,
            Ecosystem::App,
            Ecosystem::Iot,
            Ecosystem::Lib,
        ]
    }

    /// Human-readable label shown in `mg init` menu
    pub fn label(&self) -> &'static str {
        match self {
            Ecosystem::Web   => "🌐  Web application (npm/pnpm-compatible)",
            Ecosystem::Ai    => "🤖  AI agent / ML project (PyPI)",
            Ecosystem::Game  => "🎮  Game (Bevy, Unity, Unreal, Godot)",
            Ecosystem::Cloud => "☁️   Cloud infrastructure (Pulumi, Terraform)",
            Ecosystem::Cicd  => "🔄  CI/CD pipeline (GitHub Actions, App Store)",
            Ecosystem::App   => "📱  Mobile/Desktop app (Flutter, Swift, Kotlin)",
            Ecosystem::Iot   => "🔌  IoT / Embedded device",
            Ecosystem::Lib   => "📦  Library (Rust / TypeScript / Python)",
        }
    }

    /// Short name used in `mg create-<name>`
    pub fn short_name(&self) -> &'static str {
        match self {
            Ecosystem::Web   => "web",
            Ecosystem::Ai    => "ai",
            Ecosystem::Game  => "game",
            Ecosystem::Cloud => "clo",
            Ecosystem::Cicd  => "cicd",
            Ecosystem::App   => "app",
            Ecosystem::Iot   => "iot",
            Ecosystem::Lib   => "lib",
        }
    }

    /// Parse from short name string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "web"   => Some(Ecosystem::Web),
            "ai"    => Some(Ecosystem::Ai),
            "game"  => Some(Ecosystem::Game),
            "clo"   => Some(Ecosystem::Cloud),
            "cloud" => Some(Ecosystem::Cloud),
            "cicd"  => Some(Ecosystem::Cicd),
            "app"   => Some(Ecosystem::App),
            "iot"   => Some(Ecosystem::Iot),
            "lib"   => Some(Ecosystem::Lib),
            _       => None,
        }
    }
}

impl fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short_name())
    }
}
