/// Project-level configuration (mgc.toml)
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

/// Project config saved by `mgc init` and read by all commands.
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
    /// Registry configuration (mgc.toml [registries])
    #[serde(default)]
    pub registries: Vec<Registry>,
    /// Package patches (mgc.toml [patches])
    #[serde(default)]
    pub patches: Vec<mgc_types::PatchSpec>,
    /// Dedupe settings (mgc.toml [dedupe]) — opt-in (02 §2.1)
    #[serde(default)]
    pub dedupe: DedupeConfig,
    /// Library core config (mgc.toml [lib]) — ngôn ngữ + pip allowlist (Q9/Q19)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lib: Option<LibConfig>,
    /// Game core config (mgc.toml [game]) — engine (Q15)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameConfig>,
    /// IoT core config (mgc.toml [iot]) — framework + board (Q16/Q20)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iot: Option<IotConfig>,
    /// Cloud core config (mgc.toml [cloud]) — type (Q17)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudConfig>,
    /// CICD core config (mgc.toml [cicd]) — provider (Q12)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cicd: Option<CicdConfig>,
    /// App core config (mgc.toml [app]) — language (Q18)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppConfig>,
    /// AI core config (mgc.toml [ai]) — framework (Q11)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiConfig>,
    /// Security config (mgc.toml [security]) — min_release_age per ecosystem
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,
}

/// AI core config — `[ai] framework` (python-agent/mcp-server).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiConfig {
    #[serde(default)]
    pub framework: String,
}

/// App core config — `[app] language` (flutter/kotlin/swift/multi) + platforms (multi).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub language: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
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

/// Dedupe config — opt-in via mgc.toml `[dedupe] prefer = true` (02 §2.1).
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
            ai: None,
            security: None,
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
            let language = frameworks
                .first()
                .cloned()
                .unwrap_or_else(|| "flutter".to_string());
            let platforms = if language == "multi" {
                let rest: Vec<String> = frameworks.get(1..).map(|f| f.to_vec()).unwrap_or_default();
                if rest.is_empty() {
                    vec![
                        "android".to_string(),
                        "ios".to_string(),
                        "react-native".to_string(),
                        "flutter".to_string(),
                    ]
                } else {
                    rest
                }
            } else {
                Vec::new()
            };
            Some(AppConfig {
                language,
                platforms,
            })
        } else {
            None
        };
        let ai = if ecosystem == "ai" {
            Some(AiConfig {
                framework: frameworks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "python-agent".to_string()),
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
            ai,
            security: None,
        }
    }

    /// Load from project root (mgc.toml)
    pub fn load(project_root: &Path) -> Result<Option<Self>, anyhow::Error> {
        let path = project_root.join("mgc.toml");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(Some(toml::from_str(&content)?))
    }

    /// Save to project root (mgc.toml)
    pub fn save(&self, project_root: &Path) -> Result<(), anyhow::Error> {
        let path = project_root.join("mgc.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        self.write_core_marker(project_root)?;
        Ok(())
    }

    // ── Core signature marker (T9a — chống nhầm core, user 2026-08-19) ──
    // Marker file luôn đi kèm mgc.toml; sinh trong save() — mọi path (init,
    // wizard, create-*) nhận marker tự động.

    /// Core signature marker file name (const tập trung — RULE §12).
    pub const CORE_MARKER_FILE: &str = ".mgc.core";

    /// Core names accepted by the marker (chuẩn hóa "cloud" → "clo").
    pub const KNOWN_CORES: &[&str] = &[
        "web", "game", "ai", "clo", "cicd", "iot", "app", "lib", "hardware",
    ];

    /// Canonicalize a core name (trim, lowercase, alias mapping).
    fn canonical_core(name: &str) -> String {
        let n = name.trim().to_ascii_lowercase();
        if n == "cloud" {
            "clo".to_string()
        } else {
            n
        }
    }

    fn is_known_core(name: &str) -> bool {
        Self::KNOWN_CORES.contains(&name)
    }

    /// Read core marker from project root.
    ///
    /// - None: marker file does not exist.
    /// - Err: marker exists but the core name is unknown/empty (fail-closed —
    ///   never guess a wrong core from a broken signature).
    pub fn read_core_marker(project_root: &Path) -> Result<Option<String>, anyhow::Error> {
        let path = project_root.join(Self::CORE_MARKER_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        let first_line = content
            .lines()
            .next()
            .map(|l| l.split('#').next().unwrap_or("").trim())
            .unwrap_or("");
        let core = Self::canonical_core(first_line);
        if core.is_empty() || !Self::is_known_core(&core) {
            anyhow::bail!(
                "'{}' has an invalid core signature '{}'. Expected one of: {}. Fix the file or run 'mgc init --signature <core>'.",
                path.display(),
                first_line,
                Self::KNOWN_CORES.join(", "),
            );
        }
        Ok(Some(core))
    }

    /// Write core marker file (1 line plain text + optional comment).
    pub fn write_core_marker(&self, project_root: &Path) -> Result<(), anyhow::Error> {
        Self::write_core_marker_at(project_root, &self.ecosystem)
    }

    /// Write marker for an arbitrary core name (dùng cho `mgc init --signature`).
    pub fn write_core_marker_at(project_root: &Path, core: &str) -> Result<(), anyhow::Error> {
        let canonical = Self::canonical_core(core);
        if !Self::is_known_core(&canonical) {
            anyhow::bail!(
                "Unknown core '{}'. Expected one of: {}.",
                core,
                Self::KNOWN_CORES.join(", "),
            );
        }
        std::fs::create_dir_all(project_root)?;
        let path = project_root.join(Self::CORE_MARKER_FILE);
        std::fs::write(path, format!("{canonical}\n"))?;
        Ok(())
    }

    /// Collect distinct cores detected from project signature files.
    fn detect_signatures(project_root: &Path) -> Vec<String> {
        let mut cores: Vec<String> = Vec::new();
        if project_root.join("package.json").exists() {
            cores.push("web".to_string());
        }
        if project_root.join("Cargo.toml").exists() {
            cores.push("lib".to_string());
        }
        if project_root.join("pyproject.toml").exists() {
            cores.push("ai".to_string());
        }
        if project_root.join("pubspec.yaml").exists() {
            cores.push("app".to_string());
        }
        if project_root.join("Package.swift").exists() {
            cores.push("app".to_string());
        }
        cores.sort();
        cores.dedup();
        cores
    }

    /// Detect ecosystem with T9a priority: marker → mgc.toml → signatures.
    ///
    /// Err = ambiguous: multiple signature files pointing at different cores
    /// and no marker (fail-closed — never guess, RULE §9.3).
    pub fn detect_core(project_root: &Path) -> Result<Option<String>, anyhow::Error> {
        if let Some(marker) = Self::read_core_marker(project_root)? {
            return Ok(Some(marker));
        }
        if project_root.join("mgc.toml").exists() {
            return Ok(Self::read_to_string_ecosystem(project_root));
        }
        let signatures = Self::detect_signatures(project_root);
        match signatures.len() {
            0 => Ok(None),
            1 => Ok(Some(signatures[0].clone())),
            _ => anyhow::bail!(
                "Ambiguous project core in '{}': multiple signatures ({}) but no '{}'. Run 'mgc init --signature <core>' to mark the core explicitly.",
                project_root.display(),
                signatures.join(", "),
                Self::CORE_MARKER_FILE,
            ),
        }
    }

    /// Legacy detect (kept for tests): detect_core result flattened, marker
    /// first, else mgc.toml, else first signature. Luồng mới dùng detect_core.
    pub fn auto_detect(project_root: &Path) -> Option<String> {
        Self::detect_core(project_root).ok().flatten()
    }

    /// Read `[ecosystem]` from an existing mgc.toml (multi/app/adapter config priority).
    fn read_to_string_ecosystem(project_root: &Path) -> Option<String> {
        let content = std::fs::read_to_string(project_root.join("mgc.toml")).ok()?;
        let v: toml::Value = toml::from_str(&content).ok()?;
        v.get("ecosystem")
            .and_then(|e| e.as_str())
            .map(String::from)
    }

    /// Find project root by looking for mgc.toml / .mgc.core / package.json /
    /// Cargo.toml.
    ///
    /// - `mgc.toml` + `.mgc.core` checked in CWD and ALL parent directories
    ///   (monorepo support — T9a signature leo parent).
    /// - `package.json` / `Cargo.toml` etc. checked in CWD ONLY.
    pub fn find_project_root(from: &Path) -> Option<PathBuf> {
        if from.join(Self::CORE_MARKER_FILE).exists()
            || from.join("mgc.toml").exists()
            || from.join("package.json").exists()
            || from.join("Cargo.toml").exists()
            || from.join("Package.swift").exists()
            || from.join("pyproject.toml").exists()
        {
            return Some(from.to_path_buf());
        }

        let mut current = from.parent();
        while let Some(dir) = current {
            if dir.join(Self::CORE_MARKER_FILE).exists() || dir.join("mgc.toml").exists() {
                return Some(dir.to_path_buf());
            }
            current = dir.parent();
        }

        None
    }
}

/// Security config — `[security] min_release_age` per ecosystem (quarantine guard).
/// Bảo mật — `[security] min_release_age` theo ecosystem (guard cách ly).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityConfig {
    /// Minimum release age in seconds (global default) — Tuổi tối thiểu gói phát hành (mặc định toàn cục)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_release_age: Option<u64>,

    /// Per-ecosystem min_release_age overrides — Ghi đè min_release_age theo ecosystem
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lib: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iot: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cicd: Option<u64>,
}

impl SecurityConfig {
    /// Get min_release_age for specific ecosystem (fallback to global) — Lấy min_release_age cho ecosystem cụ thể
    pub fn min_age_for_ecosystem(&self, ecosystem: &str) -> Option<u64> {
        match ecosystem {
            "web" => self.web.or(self.min_release_age),
            "ai" => self.ai.or(self.min_release_age),
            "app" => self.app.or(self.min_release_age),
            "lib" => self.lib.or(self.min_release_age),
            "game" => self.game.or(self.min_release_age),
            "iot" => self.iot.or(self.min_release_age),
            "cloud" => self.cloud.or(self.min_release_age),
            "cicd" => self.cicd.or(self.min_release_age),
            _ => self.min_release_age,
        }
    }
}
