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
    /// Lock config (mgc.toml [lock]) — signature policy, writer lock
    /// timeouts (V1.2 lock v4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<LockConfig>,
    /// Trust roots (mgc.toml [trust]) — key ids accepted in `require` mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<TrustConfig>,
    /// Compatibility opt-ins (mgc.toml [compat]) — explicit escape hatches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compat: Option<CompatConfig>,
    /// Registry/index sources (mgc.toml [[sources]]) — multi-index
    /// source-selection policy (design §5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<SourceConfig>>,
    /// Lifecycle script policy (mgc.toml [scripts]) — committed,
    /// reviewable per-package allow/deny + default policy. Merges with
    /// the machine-local trust DB (deny wins everywhere).
    /// (Policy script lifecycle commit được — hợp nhất với trust DB.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<ScriptsPolicy>,
}

/// Exact pre-operation snapshot of MagiCore-owned project identity files.
/// Snapshot nguyên byte các file định danh project do MGC sở hữu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIdentitySnapshot {
    marker: Option<String>,
    config: Option<String>,
}

/// Capture the marker and project config before running untrusted lifecycle hooks.
/// Chụp marker và cấu hình trước khi chạy lifecycle hook không đáng tin.
pub fn snapshot_project_identity(project_root: &Path) -> anyhow::Result<ProjectIdentitySnapshot> {
    Ok(ProjectIdentitySnapshot {
        marker: read_regular_project_text(
            &project_root.join(ProjectConfig::CORE_MARKER_FILE),
            "core marker",
        )?,
        config: read_regular_project_text(&project_root.join("mgc.toml"), "project config")?,
    })
}

/// Check the marker bytes and canonical configured core against a snapshot.
/// So marker nguyên byte và core canonical trong config khớp snapshot.
pub fn project_identity_matches_snapshot(
    project_root: &Path,
    snapshot: &ProjectIdentitySnapshot,
) -> anyhow::Result<bool> {
    let marker = read_regular_project_text(
        &project_root.join(ProjectConfig::CORE_MARKER_FILE),
        "core marker",
    )?;
    if marker != snapshot.marker {
        return Ok(false);
    }
    let current_config =
        read_regular_project_text(&project_root.join("mgc.toml"), "project config")?;
    let current_core = ecosystem_from_config(current_config.as_deref())?;
    let expected_core = ecosystem_from_config(snapshot.config.as_deref())?;
    Ok(current_core.as_deref().map(ProjectConfig::canonical_core)
        == expected_core.as_deref().map(ProjectConfig::canonical_core))
}

/// Restore identity files atomically after a lifecycle hook changes ownership.
/// Refuses symlink/special-file targets and reports every failed restoration.
/// Khôi phục atomic file identity sau khi hook đổi ownership; từ chối symlink/file đặc biệt.
pub fn restore_project_identity(
    project_root: &Path,
    snapshot: &ProjectIdentitySnapshot,
) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for (path, content, label) in [
        (
            project_root.join("mgc.toml"),
            snapshot.config.as_deref(),
            "project config",
        ),
        (
            project_root.join(ProjectConfig::CORE_MARKER_FILE),
            snapshot.marker.as_deref(),
            "core marker",
        ),
    ] {
        let result = read_regular_project_text(&path, label).and_then(|current| {
            let unchanged = if label == "project config" {
                match (
                    ecosystem_from_config(current.as_deref()),
                    ecosystem_from_config(content),
                ) {
                    (Ok(current), Ok(expected)) => {
                        current.as_deref().map(ProjectConfig::canonical_core)
                            == expected.as_deref().map(ProjectConfig::canonical_core)
                    }
                    _ => false,
                }
            } else {
                current.as_deref() == content
            };
            if unchanged {
                return Ok(());
            }
            match content {
                Some(content) => atomic_write_project_config(&path, content.as_bytes()),
                None => remove_regular_project_identity_file(&path),
            }
        });
        if let Err(error) = result {
            failures.push(format!("restore {label} '{}': {error}", path.display()));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "project identity restoration failed: {}",
            failures.join("; ")
        )
    }
}

fn remove_regular_project_identity_file(path: &Path) -> anyhow::Result<()> {
    if ensure_regular_project_config(path)?.is_none() {
        return Ok(());
    }
    std::fs::remove_file(path)?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

/// Lifecycle script policy — `[scripts] policy/allow/deny`.
/// npm parity (approve-scripts/deny-scripts) in committed form: the local
/// trust DB stays machine-private, this file is reviewed like code.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScriptsPolicy {
    /// Default for packages with scripts but no explicit entry:
    /// `allow` (current behavior), `deny`, or `prompt` (deny + hint).
    /// (Mặc định cho package chưa có entry.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// Explicitly allowed package names (exact or `name@version`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    /// Explicitly denied package names (exact or `name@version`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
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
            lock: None,
            trust: None,
            compat: None,
            sources: None,
            scripts: None,
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
                        "go" | "golang" => "go",
                        "dotnet" | "csharp" | "c#" => "dotnet",
                        "java" | "maven" => "java",
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
            lock: None,
            trust: None,
            compat: None,
            sources: None,
            scripts: None,
        }
    }

    /// Load from project root (mgc.toml)
    pub fn load(project_root: &Path) -> Result<Option<Self>, anyhow::Error> {
        let path = project_root.join("mgc.toml");
        let Some(content) = read_regular_project_text(&path, "project config")? else {
            return Ok(None);
        };
        Ok(Some(toml::from_str(&content)?))
    }

    /// Save to project root (mgc.toml)
    pub fn save(&self, project_root: &Path) -> Result<(), anyhow::Error> {
        let desired_core = Self::canonical_core(&self.ecosystem);
        // Claim the project core before writing config so a save from another
        // core cannot replace either the marker or the existing mgc.toml.
        // (Ghi nhận core trước để save từ core khác không thể thay marker hay mgc.toml.)
        Self::ensure_core_marker_at(project_root, &desired_core)?;
        let path = project_root.join("mgc.toml");
        let content = toml::to_string_pretty(self)?;
        atomic_write_project_config(&path, content.as_bytes())?;
        Ok(())
    }

    fn ensure_mgc_config_core_compatible(
        project_root: &Path,
        desired_core: &str,
    ) -> Result<(), anyhow::Error> {
        let path = project_root.join("mgc.toml");
        if ensure_regular_project_config(&path)?.is_none() {
            return Ok(());
        }
        let Some(existing) = Self::load(project_root)? else {
            return Ok(());
        };
        let existing_core = Self::canonical_core(&existing.ecosystem);
        if existing_core != desired_core {
            anyhow::bail!(
                "Project is already configured as core '{}'; refusing to replace it with '{}'. Core reassignment is not supported by this version.",
                existing_core,
                desired_core,
            );
        }
        Ok(())
    }

    // ── Plain-text core identity marker (T9a — chống nhầm core) ──
    // Marker file luôn đi kèm mgc.toml; sinh trong save() — mọi path (init,
    // wizard, create-*) nhận marker tự động.

    /// Plain-text core identity marker file name — not a cryptographic signature.
    pub const CORE_MARKER_FILE: &str = ".mgc.core";

    /// Core names accepted by the marker (chuẩn hóa "cloud" → "clo").
    pub const KNOWN_CORES: &[&str] = &[
        "web", "game", "ai", "clo", "cicd", "iot", "app", "lib", "hardware",
    ];

    /// Canonicalize a core name (trim, lowercase, alias mapping).
    fn canonical_core(name: &str) -> String {
        let n = name.trim().to_ascii_lowercase();
        if n == "cloud" { "clo".to_string() } else { n }
    }

    fn is_known_core(name: &str) -> bool {
        Self::KNOWN_CORES.contains(&name)
    }

    /// Read core marker from project root.
    ///
    /// - None: marker file does not exist.
    /// - Err: marker exists but the core name is unknown/empty (fail-closed —
    ///   never guess a wrong core from a malformed marker).
    pub fn read_core_marker(project_root: &Path) -> Result<Option<String>, anyhow::Error> {
        let path = project_root.join(Self::CORE_MARKER_FILE);
        let Some(content) = read_regular_project_text(&path, "core marker")? else {
            return Ok(None);
        };
        let first_line = content
            .lines()
            .next()
            .map(|l| l.split('#').next().unwrap_or("").trim())
            .unwrap_or("");
        let core = Self::canonical_core(first_line);
        if core.is_empty() || !Self::is_known_core(&core) {
            anyhow::bail!(
                "'{}' has an invalid core marker '{}'. Expected one of: {}. Repair the marker file manually before running core-aware commands.",
                path.display(),
                first_line,
                Self::KNOWN_CORES.join(", "),
            );
        }
        Self::validate_marker_matches_project_config(project_root, &core)?;
        Ok(Some(core))
    }

    /// Reject a marker that disagrees with the persisted project identity.
    /// Từ chối marker lệch với identity đã lưu trong cấu hình project.
    fn validate_marker_matches_project_config(
        project_root: &Path,
        marker: &str,
    ) -> Result<(), anyhow::Error> {
        if ensure_regular_project_config(&project_root.join("mgc.toml"))?.is_none() {
            return Ok(());
        }
        let configured = Self::read_to_string_ecosystem(project_root)?
            .ok_or_else(|| anyhow::anyhow!("project config is missing its ecosystem"))?;
        let configured = Self::canonical_core(&configured);
        if !Self::is_known_core(&configured) {
            anyhow::bail!(
                "project config has unknown core '{}'; refusing to trust core marker '{}'",
                configured,
                marker,
            );
        }
        if configured != marker {
            anyhow::bail!(
                "core marker '{}' conflicts with mgc.toml core '{}'; core reassignment is not supported by this version",
                marker,
                configured,
            );
        }
        Ok(())
    }

    /// Write core marker file (1 line plain text + optional comment).
    pub fn write_core_marker(&self, project_root: &Path) -> Result<(), anyhow::Error> {
        Self::ensure_core_marker_at(project_root, &self.ecosystem)
    }

    /// Claim an absent marker or verify an existing marker has the same core.
    /// Nhận ownership nếu marker chưa có; marker khác core luôn bị từ chối.
    pub fn ensure_core_marker_at(project_root: &Path, core: &str) -> Result<(), anyhow::Error> {
        let canonical = Self::canonical_core(core);
        if !Self::is_known_core(&canonical) {
            anyhow::bail!(
                "Unknown core '{}'. Expected one of: {}.",
                core,
                Self::KNOWN_CORES.join(", "),
            );
        }
        // Keep the compatibility check in the shared ownership primitive so
        // every public entry point (including `write_core_marker`) is unable
        // to override an existing mgc.toml core identity.
        // (Đặt kiểm tra ở primitive chung để mọi API public đều không thể ghi đè core.)
        Self::ensure_mgc_config_core_compatible(project_root, &canonical)?;
        std::fs::create_dir_all(project_root)?;
        let path = project_root.join(Self::CORE_MARKER_FILE);
        match Self::read_core_marker(project_root)? {
            Some(existing) if existing == canonical => return Ok(()),
            Some(existing) => anyhow::bail!(
                "Project is already marked as core '{}'; refusing to replace it with '{}'. Core reassignment is not supported by this version.",
                existing,
                canonical,
            ),
            None => {}
        }

        // Publish a fully written marker with a no-replace hard link. Creating
        // the destination first and filling it afterward exposed an empty or
        // partial identity file if the process was killed between those steps.
        // (Publish marker đã ghi hoàn chỉnh bằng hard link không-ghi-đè; tránh
        // marker rỗng/nửa chừng nếu process bị kill.)
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let temp_path = parent.join(format!(
            ".{}.claim.{}.{}",
            Self::CORE_MARKER_FILE,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let result = (|| -> anyhow::Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
            }
            let mut file = options.open(&temp_path)?;
            use std::io::Write;
            file.write_all(format!("{canonical}\n").as_bytes())?;
            file.sync_all()?;
            match std::fs::hard_link(&temp_path, &path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return match Self::read_core_marker(project_root)? {
                        Some(existing) if existing == canonical => Ok(()),
                        Some(existing) => anyhow::bail!(
                            "Project is already marked as core '{}'; refusing to replace it with '{}'.",
                            existing,
                            canonical,
                        ),
                        None => anyhow::bail!(
                            "Core marker '{}' changed while claiming project ownership; retry the command.",
                            path.display(),
                        ),
                    };
                }
                Err(error) => return Err(error.into()),
            }
            #[cfg(unix)]
            std::fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let cleanup = std::fs::remove_file(&temp_path);
        if let Err(error) = result {
            let _ = cleanup;
            return Err(error);
        }
        cleanup?;
        Ok(())
    }

    /// Write a core marker only when it agrees with any existing project config.
    /// Chỉ ghi marker khi nhất quán với cấu hình project hiện có.
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
        Self::ensure_core_marker_at(project_root, &canonical)
    }

    /// Collect distinct cores detected from project manifests.
    fn detect_signatures(project_root: &Path) -> Result<Vec<String>, anyhow::Error> {
        let mut cores: Vec<String> = Vec::new();
        if signature_file_exists(&project_root.join("package.json"))? {
            cores.push("web".to_string());
        }
        if signature_file_exists(&project_root.join("Cargo.toml"))? {
            cores.push("lib".to_string());
        }
        if signature_file_exists(&project_root.join("pyproject.toml"))? {
            cores.push("ai".to_string());
        }
        if signature_file_exists(&project_root.join("pubspec.yaml"))? {
            cores.push("app".to_string());
        }
        if signature_file_exists(&project_root.join("Package.swift"))? {
            cores.push("app".to_string());
        }
        cores.sort();
        cores.dedup();
        Ok(cores)
    }

    /// Detect ecosystem with T9a priority: marker → mgc.toml → signatures.
    ///
    /// Err = ambiguous: multiple ecosystem manifests point at different cores
    /// and no marker (fail-closed — never guess, RULE §9.3).
    pub fn detect_core(project_root: &Path) -> Result<Option<String>, anyhow::Error> {
        if let Some(marker) = Self::read_core_marker(project_root)? {
            return Ok(Some(marker));
        }
        if ensure_regular_project_config(&project_root.join("mgc.toml"))?.is_some() {
            return Self::read_to_string_ecosystem(project_root);
        }
        let signatures = Self::detect_signatures(project_root)?;
        match signatures.len() {
            0 => Ok(None),
            1 => Ok(Some(signatures[0].clone())),
            _ => anyhow::bail!(
                "Ambiguous project core in '{}': multiple ecosystem manifests ({}) but no '{}'. Run 'mgc init --signature <core>' to write a plain-text core marker explicitly.",
                project_root.display(),
                signatures.join(", "),
                Self::CORE_MARKER_FILE,
            ),
        }
    }

    /// Detect the project core without hiding malformed or conflicting identity errors.
    /// Phát hiện core nhưng không biến marker hỏng/xung đột thành kết quả không tìm thấy.
    pub fn auto_detect(project_root: &Path) -> Result<Option<String>, anyhow::Error> {
        Self::detect_core(project_root)
    }

    /// Read `[ecosystem]` from an existing mgc.toml (multi/app/adapter config priority).
    fn read_to_string_ecosystem(project_root: &Path) -> Result<Option<String>, anyhow::Error> {
        let Some(content) =
            read_regular_project_text(&project_root.join("mgc.toml"), "project config")?
        else {
            return Ok(None);
        };
        ecosystem_from_config(Some(&content))
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

fn ecosystem_from_config(content: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(content) = content else {
        return Ok(None);
    };
    let value: toml::Value = toml::from_str(content)?;
    Ok(value
        .get("ecosystem")
        .and_then(|ecosystem| ecosystem.as_str())
        .map(String::from))
}

/// Return metadata only for an existing regular config file. Symlinks and
/// special files are refused before project identity is read or mutated.
/// Chỉ trả metadata của config file thường; từ chối symlink/file đặc biệt.
fn ensure_regular_project_config(path: &Path) -> anyhow::Result<Option<std::fs::Metadata>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!("project config '{}' must not be a symlink", path.display());
    }
    if !metadata.is_file() {
        anyhow::bail!("project config '{}' must be a regular file", path.display());
    }
    Ok(Some(metadata))
}

/// Read a project identity/configuration file from a no-follow handle.
/// Path metadata is checked for clear diagnostics, then the opened handle is
/// checked again so a final-component symlink swap cannot redirect the read.
/// Đọc file identity/config từ handle no-follow; kiểm tra lại metadata handle
/// để symlink swap ở thành phần cuối không thể đổi đích đọc.
/// Read a regular project file without following the final path component.
/// Symlinks, reparse points, and special files are rejected. Callers should
/// parse the returned text with the format-specific parser they own.
/// Đọc file thường trong project, không đi theo path component cuối.
pub fn read_regular_project_text(path: &Path, label: &str) -> anyhow::Result<Option<String>> {
    use std::io::Read;

    const MAX_PROJECT_TEXT_BYTES: u64 = 10 * 1024 * 1024;

    let path_metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if path_metadata.file_type().is_symlink() {
        anyhow::bail!("{label} '{}' must not be a symlink", path.display());
    }
    if !path_metadata.is_file() {
        anyhow::bail!("{label} '{}' must be a regular file", path.display());
    }

    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|error| {
        anyhow::anyhow!(
            "open {label} '{}' without following links: {error}",
            path.display()
        )
    })?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        anyhow::bail!("{label} '{}' is not a regular file", path.display());
    }
    if opened_metadata.len() > MAX_PROJECT_TEXT_BYTES {
        anyhow::bail!(
            "{label} '{}' exceeds the {} byte safety limit",
            path.display(),
            MAX_PROJECT_TEXT_BYTES
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if opened_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            anyhow::bail!("{label} '{}' must not be a reparse point", path.display());
        }
    }

    let mut content = String::new();
    file.take(MAX_PROJECT_TEXT_BYTES + 1)
        .read_to_string(&mut content)?;
    if content.len() as u64 > MAX_PROJECT_TEXT_BYTES {
        anyhow::bail!(
            "{label} '{}' exceeds the {} byte safety limit",
            path.display(),
            MAX_PROJECT_TEXT_BYTES
        );
    }
    Ok(Some(content))
}

/// A project signature is only recognized when it is a regular file owned
/// by the project tree; symlinked or special-file signatures fail closed.
/// Chỉ nhận signature là file thường trong project; symlink/file đặc biệt bị từ chối.
fn signature_file_exists(path: &Path) -> anyhow::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "project signature '{}' must not be a symlink",
                path.display()
            )
        }
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => anyhow::bail!(
            "project signature '{}' must be a regular file",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Publish `mgc.toml` through a unique same-directory staging file. Replacing
/// the path atomically avoids truncation and never follows a swapped symlink.
/// Publish `mgc.toml` qua staging file cùng thư mục, atomic, không truncate.
fn atomic_write_project_config(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("project config path has no file name"))?;
    let metadata = ensure_regular_project_config(path)?;
    let temp_path = parent.join(format!(
        ".{}.tmp.{}.{}",
        file_name.to_string_lossy(),
        std::process::id(),
        uuid::Uuid::new_v4()
    ));

    let result = (|| -> anyhow::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&temp_path)?;
        if let Some(metadata) = &metadata {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        ensure_regular_project_config(path)?;
        mgc_lockfile::atomic::atomic_replace_file(&temp_path, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Security config — `[security] min_release_age` per ecosystem (quarantine guard).
/// Bảo mật — `[security] min_release_age` theo ecosystem (guard cách ly).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityConfig {
    /// Minimum release age in HOURS (global default) — Tuổi tối thiểu gói phát hành, tính bằng GIỜ (mặc định toàn cục)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_release_age: Option<u64>,
    /// When true, versions with missing/unparsable registry timestamps
    /// are KEPT (fail-open) — escape hatch for private registries that
    /// omit `time`. Default false: unstamped versions are rejected with
    /// a clear error naming the package (fail-closed).
    /// (Cho phép version thiếu timestamp — escape hatch cho registry nội bộ.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_missing_time: Option<bool>,

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

    /// Explicit per-ecosystem unsigned-artifact escape (`"*" rejected —
    /// see `unsigned_artifact_allowed`). Wired into lanes in Phase C;
    /// today the resolver denies unsigned artifacts unconditionally.
    /// (Escape artifact-không-chữ-ký per-ecosystem tường minh.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_unsigned_artifacts: Option<Vec<String>>,
}

/// Check an unsigned-artifact escape list (`mgc.toml [security]
/// allow_unsigned_artifacts`, mirrored by `MGC_ALLOW_UNSIGNED_ARTIFACTS`
/// comma env): explicit per-ecosystem opt-outs only — `"*"` is rejected
/// as a value (a global opt-out must be a deliberate empty-vs-absent
/// decision at the call site, never a wildcard in config).
/// (Kiểm tra danh sách escape artifact-không-chữ-ký: chỉ opt-out
/// per-ecosystem tường minh — từ chối `"*"`.)
pub fn unsigned_artifact_allowed(allowed: &[String], ecosystem: &str) -> bool {
    allowed
        .iter()
        .any(|entry| entry.trim().eq_ignore_ascii_case(ecosystem))
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

/// Lock config — `mgc.toml [lock]` (V1.2 lock v4: signature policy +
/// writer-lock timeouts). All fields optional; absences fall back to
/// environment-aware defaults (policy.rs).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockConfig {
    /// Signature policy: `off` | `warn` | `require`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// Writer-lock acquire timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquire_timeout_ms: Option<u64>,
    /// Stale-temp grace period in seconds before cleanup may unlink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmp_grace_secs: Option<u64>,
}

/// Trust roots — `mgc.toml [trust]`: key ids accepted when the lock
/// policy is `require` (local keyrings never qualify on their own).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustConfig {
    /// Accepted signing key ids (8-byte BLAKE3 hex).
    #[serde(default)]
    pub keys: Vec<String>,
}

/// Compatibility opt-ins — `mgc.toml [compat]`: explicit escape hatches
/// (default-deny everything else).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatConfig {
    /// Allow git-dependency resolution at all (Swift git-only deps).
    #[serde(default)]
    pub allow_git_deps: bool,
    /// Host allowlist for git dependencies.
    #[serde(default)]
    pub git_hosts: Vec<String>,
}

/// Registry/index source — `mgc.toml [[sources]]` (design §5
/// multi-index source-selection policy).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceConfig {
    /// Stable id referenced by lockfile source_id.
    pub id: String,
    /// Base URL.
    pub url: String,
    /// Ecosystem served (`python`, `npm`, …).
    #[serde(default)]
    pub ecosystem: String,
    /// Lower number wins among equal-specificity claims.
    #[serde(default)]
    pub priority: u64,
    /// Name patterns claimed (`*`, `corp-*`, `@corp/*`, exact).
    #[serde(default)]
    pub claims: Vec<String>,
    /// Private/internal source (never overridden by public ones).
    #[serde(default)]
    pub trusted: bool,
    /// Exact hosts allowed when trusted.
    #[serde(default)]
    pub allow_hosts: Vec<String>,
    /// Private CIDRs allowed when trusted.
    #[serde(default)]
    pub allow_cidrs: Vec<String>,
    /// URL schemes allowed when trusted.
    #[serde(default)]
    pub allow_protocols: Vec<String>,
}

/// True when a package (by `name` or `name@version`) is listed in a
/// policy entry list. Shared by `ScriptsPolicy::decide` and
/// `decide_scripts` so matching semantics cannot drift.
/// (Package có trong danh sách không — dùng chung mọi nơi.)
pub fn policy_lists(entries: &[String], name: &str, version: &str) -> bool {
    entries.iter().any(|e| {
        let entry = e.trim();
        entry == name || entry == format!("{name}@{version}").as_str()
    })
}

impl ScriptsPolicy {
    /// Committed file-policy decision for one package: deny wins, then
    /// allow, else no opinion. Single canonical rule shared by the
    /// install gate and `trust pending` so they can never disagree.
    /// (Quyết định policy file chuẩn duy nhất cho gate install và trust pending.)
    pub fn decide(&self, name: &str, version: &str) -> Option<(bool, &'static str)> {
        if policy_lists(&self.deny, name, version) {
            return Some((false, "mgc.toml [scripts] deny"));
        }
        if policy_lists(&self.allow, name, version) {
            return Some((true, "mgc.toml [scripts] allow"));
        }
        match self.policy.as_deref() {
            Some("deny") | Some("prompt") => Some((false, "mgc.toml [scripts] policy")),
            // Explicit `policy = "allow"` allows everything not denied
            // above (documented behavior — absence of the key means "no
            // opinion", an explicit allow-all is a deliberate choice).
            // (`policy = "allow"` tường minh cho phép tất cả.)
            Some("allow") => Some((true, "mgc.toml [scripts] policy")),
            _ => None,
        }
    }
}

/// One merged lifecycle-script decision for a package, combining the
/// committed file policy and the machine-local trust DB. Deny wins from
/// EITHER source; then allow from either source; otherwise undecided
/// (the caller maps undecided to its own default: skip-with-hint on
/// install, pending-review on `trust pending`).
/// (Một quyết định gộp duy nhất — deny luôn thắng.)
pub enum ScriptVerdict {
    /// Scripts must not run (reason names the winning deny source).
    Deny(&'static str),
    /// Scripts may run (reason names the winning allow source).
    Allow(&'static str),
    /// Neither source has an opinion.
    Undecided,
}

/// Single function used by the install gate AND `trust pending` — two
/// call sites can never drift again.
/// (Hàm duy nhất cho gate install VÀ trust pending.)
pub fn decide_scripts(
    name: &str,
    version: &str,
    file_policy: Option<&ScriptsPolicy>,
    db_policy: Option<&str>,
    blanket_scripts: bool,
) -> ScriptVerdict {
    // Deny from either source wins outright (a file allow must NEVER
    // override a DB deny — that drift shipped once and is covered by a
    // regression test).
    // (Deny thắng mọi nguồn — file allow không bao giờ ghi đè DB deny.)
    let file_deny = file_policy
        .map(|policy| policy_lists(&policy.deny, name, version))
        .unwrap_or(false);
    if file_deny {
        return ScriptVerdict::Deny("mgc.toml [scripts] deny");
    }
    if db_policy == Some("denied") {
        return ScriptVerdict::Deny("mgc trust deny");
    }
    if let Some((allowed, reason)) = file_policy.and_then(|policy| policy.decide(name, version)) {
        if allowed {
            return ScriptVerdict::Allow(reason);
        }
        return ScriptVerdict::Deny(reason);
    }
    match db_policy {
        Some("approved") => ScriptVerdict::Allow("mgc trust approve"),
        _ if blanket_scripts => ScriptVerdict::Allow("default policy"),
        _ => ScriptVerdict::Undecided,
    }
}

/// Load ONLY the `[scripts]` table from a project mgc.toml, tolerating
/// minimal files that full `ProjectConfig::load` rejects (it requires
/// name/ecosystem). Returns `Ok(None)` when no table exists, `Err` with
/// a precise reason when the file/table is present but broken — callers
/// must surface the error (a silent None would drop a deny policy).
/// (Đọc riêng bảng `[scripts]` — file hỏng thì lỗi rõ, không im lặng.)
pub fn load_scripts_table(project_root: &std::path::Path) -> Result<Option<ScriptsPolicy>, String> {
    let path = project_root.join("mgc.toml");
    let Some(text) = read_regular_project_text(&path, "project config")
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
    else {
        return Ok(None);
    };
    let value: toml::Value = text
        .parse()
        .map_err(|e| format!("invalid TOML in {}: {e}", path.display()))?;
    let Some(table) = value.get("scripts") else {
        return Ok(None);
    };
    let json = serde_json::to_value(table)
        .map_err(|e| format!("invalid [scripts] table in {}: {e}", path.display()))?;
    serde_json::from_value::<ScriptsPolicy>(json)
        .map(Some)
        .map_err(|e| format!("invalid [scripts] table in {}: {e}", path.display()))
}
