/// Project-level configuration (mg.toml)
///
/// Stores ecosystem/core, scaffold settings, and per-core configuration.
use crate::registry::Registry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectExecutionConfig {
    #[serde(default = "default_execution_architecture")]
    pub architecture: String,
    #[serde(default = "default_execution_lane")]
    pub lane: String,
    #[serde(default = "default_execution_compatibility_layer")]
    pub compatibility_layer: String,
    #[serde(default)]
    pub native_targets: Vec<String>,
}

fn default_execution_architecture() -> String {
    "rust-first".to_string()
}

fn default_execution_lane() -> String {
    "compatibility-shell".to_string()
}

fn default_execution_compatibility_layer() -> String {
    "js".to_string()
}

impl Default for ProjectExecutionConfig {
    fn default() -> Self {
        Self {
            architecture: default_execution_architecture(),
            lane: default_execution_lane(),
            compatibility_layer: default_execution_compatibility_layer(),
            native_targets: vec![],
        }
    }
}

fn default_execution_for(ecosystem: &str, features: &[String]) -> ProjectExecutionConfig {
    let has_ts = features.iter().any(|feature| {
        let value = feature.trim().to_ascii_lowercase();
        value == "ts" || value == "typescript"
    });

    match ecosystem {
        "web" => ProjectExecutionConfig {
            architecture: "rust-first".to_string(),
            lane: "compatibility-shell".to_string(),
            compatibility_layer: if has_ts { "ts" } else { "js" }.to_string(),
            native_targets: vec![
                "frontend-executable".to_string(),
                "backend-executable".to_string(),
                "wasm-bridge".to_string(),
            ],
        },
        "game" | "app" | "lib" => ProjectExecutionConfig {
            architecture: "native-first".to_string(),
            lane: "native-ready".to_string(),
            compatibility_layer: "none".to_string(),
            native_targets: vec!["binary".to_string()],
        },
        _ => ProjectExecutionConfig {
            architecture: "rust-first".to_string(),
            lane: "compatibility-shell".to_string(),
            compatibility_layer: "none".to_string(),
            native_targets: vec![],
        },
    }
}

/// Project config saved by `mg init` and read by all commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name
    pub name: String,
    /// Project version
    #[serde(default = "default_version")]
    pub version: String,
    /// Ecosystem / core type (web, game, ai, cloud, iot, app, lib)
    pub ecosystem: String,
    /// Mode (frontend, backend, fullstack, monorepo) — web core only
    #[serde(default)]
    pub mode: String,
    /// Frameworks used (e.g. ["react-vite"] or ["node", "express"])
    #[serde(default)]
    pub frameworks: Vec<String>,
    /// Template path used during scaffold
    #[serde(default)]
    pub template: String,
    /// Features selected (e.g. ["ts", "tailwind"])
    #[serde(default)]
    pub features: Vec<String>,
    /// Execution strategy / runtime lane metadata
    #[serde(default)]
    pub execution: ProjectExecutionConfig,
    /// Registry configuration (mg.toml [registries])
    #[serde(default)]
    pub registries: Vec<Registry>,
    /// Package patches (mg.toml [patches])
    #[serde(default)]
    pub patches: Vec<mg_types::PatchSpec>,
    /// Dedupe settings (mg.toml [dedupe]) — opt-in (02 §2.1)
    #[serde(default)]
    pub dedupe: DedupeConfig,
    /// Library core config (mg.toml [lib]) — ngôn ngữ + pip allowlist (Q9/Q19)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lib: Option<LibConfig>,
    /// Game core config (mg.toml [game]) — engine (Q15)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameConfig>,
    /// IoT core config (mg.toml [iot]) — framework + board (Q16/Q20)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iot: Option<IotConfig>,
    /// Cloud core config (mg.toml [cloud]) — type (Q17)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudConfig>,
    /// CICD core config (mg.toml [cicd]) — provider (Q12)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cicd: Option<CicdConfig>,
    /// App core config (mg.toml [app]) — language (Q18)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppConfig>,
}

/// App core config — `[app] language` (flutter/kotlin/swift).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub language: String,
}

/// CICD core config — `[cicd] provider` (github-actions/cloudflare/aws/gcp/argocd).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CicdConfig {
    #[serde(default)]
    pub provider: String,
}

/// Cloud core config — `[cloud] type` (cdk/pulumi/terraform).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudConfig {
    #[serde(default)]
    pub r#type: String,
}

/// Game core config — `[game] engine` (bevy/godot/unity/unreal).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameConfig {
    #[serde(default)]
    pub engine: String,
}

/// IoT core config — `[iot] framework` (esp32-rust/platformio/zephyr) + `board`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IotConfig {
    #[serde(default)]
    pub framework: String,
    #[serde(default)]
    pub board: String,
}

/// Library core config — `[lib] language` (ts/rust/python) + pip package allowlist.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibConfig {
    #[serde(default)]
    pub language: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pip_allowed_packages: Vec<String>,
}

/// Dedupe config — opt-in via mg.toml `[dedupe] prefer = true` (02 §2.1).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DedupeConfig {
    #[serde(default)]
    pub prefer: bool,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_iot_board(framework: Option<&str>) -> &'static str {
    match framework {
        Some("esp32-rust") => "esp32c3",
        Some("platformio") => "esp32dev",
        _ => "nrf52dk_nrf52832",
    }
}

impl ProjectConfig {
    pub fn new(name: impl Into<String>, ecosystem: impl Into<String>) -> Self {
        let ecosystem = ecosystem.into();
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            ecosystem: ecosystem.clone(),
            mode: String::new(),
            frameworks: vec![],
            template: String::new(),
            features: vec![],
            execution: default_execution_for(&ecosystem, &[]),
            registries: vec![],
            patches: vec![],
            dedupe: DedupeConfig::default(),
            lib: None,
            game: None,
            iot: None,
            cloud: None,
            cicd: None,
            app: None,
        }
    }

    pub fn from_scaffold(
        name: impl Into<String>,
        ecosystem: impl Into<String>,
        mode: impl Into<String>,
        frameworks: Vec<String>,
        template: impl Into<String>,
        features: Vec<String>,
    ) -> Self {
        let ecosystem = ecosystem.into();
        let lib = if ecosystem == "lib" {
            Some(LibConfig {
                language: frameworks
                    .first()
                    .map(|f| match f.as_str() {
                        "typescript" | "ts" => "ts",
                        "python" | "py" => "python",
                        _ => "rust",
                    })
                    .unwrap_or("rust")
                    .to_string(),
                pip_allowed_packages: Vec::new(),
            })
        } else {
            None
        };
        let game = if ecosystem == "game" {
            Some(GameConfig {
                engine: frameworks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "bevy".to_string()),
            })
        } else {
            None
        };
        let iot = if ecosystem == "iot" {
            let framework = frameworks
                .first()
                .cloned()
                .unwrap_or_else(|| "esp32-rust".to_string());
            Some(IotConfig {
                board: features
                    .first()
                    .cloned()
                    .unwrap_or_else(|| default_iot_board(Some(&framework)).to_string()),
                framework,
            })
        } else {
            None
        };
        let cloud = if ecosystem == "clo" || ecosystem == "cloud" {
            Some(CloudConfig {
                r#type: frameworks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "terraform".to_string()),
            })
        } else {
            None
        };
        let cicd = if ecosystem == "cicd" {
            Some(CicdConfig {
                provider: frameworks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "github-actions".to_string()),
            })
        } else {
            None
        };
        let app = if ecosystem == "app" {
            Some(AppConfig {
                language: frameworks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "flutter".to_string()),
            })
        } else {
            None
        };
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            ecosystem: ecosystem.clone(),
            mode: mode.into(),
            frameworks,
            template: template.into(),
            execution: default_execution_for(&ecosystem, &features),
            features,
            registries: vec![],
            patches: vec![],
            dedupe: DedupeConfig::default(),
            lib,
            game,
            iot,
            cloud,
            cicd,
            app,
        }
    }

    /// Load from project root (mg.toml)
    pub fn load(project_root: &Path) -> Result<Option<Self>, anyhow::Error> {
        let path = project_root.join("mg.toml");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(Some(toml::from_str(&content)?))
    }

    /// Save to project root (mg.toml)
    pub fn save(&self, project_root: &Path) -> Result<(), anyhow::Error> {
        let path = project_root.join("mg.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Detect ecosystem from project files (fallback if no mg.toml)
    pub fn auto_detect(project_root: &Path) -> Option<String> {
        if project_root.join("package.json").exists() {
            return Some("web".to_string());
        }
        if project_root.join("Cargo.toml").exists() {
            return Some("lib".to_string());
        }
        if project_root.join("pyproject.toml").exists() {
            return Some("ai".to_string());
        }
        None
    }

    /// Find project root by looking for mg.toml, package.json, or Cargo.toml.
    ///
    /// - `mg.toml` checked in CWD and ALL parent directories (monorepo support).
    /// - `package.json` / `Cargo.toml` checked in CWD ONLY.
    pub fn find_project_root(from: &Path) -> Option<PathBuf> {
        if from.join("mg.toml").exists()
            || from.join("package.json").exists()
            || from.join("Cargo.toml").exists()
        {
            return Some(from.to_path_buf());
        }

        let mut current = from.parent();
        while let Some(dir) = current {
            if dir.join("mg.toml").exists() {
                return Some(dir.to_path_buf());
            }
            current = dir.parent();
        }

        None
    }
}
