//! Scaffold specification parser — template spec with version/tag support
//! Parser thống nhất cho toàn core: web, ai, app, lib
//!
//! Ví dụ:
//! - `nextjs@latest` → name="nextjs", ref=DistTag("latest")
//! - `fastapi@0.115.0` → name="fastapi", ref=Version("0.115.0")
//! - `rust@1.80` → name="rust", ref=Version("1.80")
//! - `react-vite` → name="react-vite", ref=Default

use anyhow::{bail, Result};

/// Scaffold specification đã parse
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldSpec {
    /// Core kind (web, ai, app, lib)
    pub core: CoreKind,
    /// Template name gốc (user input)
    pub name: String,
    /// Normalized name (dùng cho artifact lookup)
    pub normalized_name: String,
    /// Version/tag reference
    pub requested_ref: ScaffoldRef,
}

/// Core kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreKind {
    Web,
    Ai,
    App,
    Lib,
    Game,
    Iot,
    Cicd,
    Cloud,
}

impl CoreKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreKind::Web => "web",
            CoreKind::Ai => "ai",
            CoreKind::App => "app",
            CoreKind::Lib => "lib",
            CoreKind::Game => "game",
            CoreKind::Iot => "iot",
            CoreKind::Cicd => "cicd",
            CoreKind::Cloud => "cloud",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "web" => Some(CoreKind::Web),
            "ai" => Some(CoreKind::Ai),
            "app" => Some(CoreKind::App),
            "lib" => Some(CoreKind::Lib),
            "game" => Some(CoreKind::Game),
            "iot" => Some(CoreKind::Iot),
            "cicd" => Some(CoreKind::Cicd),
            "cloud" => Some(CoreKind::Cloud),
            _ => None,
        }
    }
}

/// Scaffold reference: tag hoặc exact version
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaffoldRef {
    /// Dist tag (e.g., "latest", "stable", "beta")
    DistTag(String),
    /// Exact version (e.g., "15.5.0", "1.80")
    Version(String),
    /// Version range (e.g., "^1.0.0") - P2, chưa support
    Range(String),
    /// Default (no version specified)
    Default,
}

impl ScaffoldRef {
    /// Check if tag/version looks valid
    pub fn validate(&self) -> Result<()> {
        match self {
            ScaffoldRef::DistTag(tag) => {
                if tag.is_empty() {
                    bail!("Empty dist tag");
                }
                // Kiểm tra typo phổ biến
                if tag == "laster" {
                    bail!("Unknown tag 'laster'. Did you mean 'latest'?");
                }
                Ok(())
            }
            ScaffoldRef::Version(ver) => {
                if ver.is_empty() {
                    bail!("Empty version");
                }
                // Basic semver check
                if !ver.chars().next().unwrap_or('0').is_ascii_digit() {
                    bail!("Version must start with digit, got '{}'", ver);
                }
                Ok(())
            }
            ScaffoldRef::Range(_) => {
                bail!("Version ranges not supported yet");
            }
            ScaffoldRef::Default => Ok(()),
        }
    }

    /// Suggest common typos
    pub fn suggest_if_typo(&self) -> Option<String> {
        match self {
            ScaffoldRef::DistTag(tag) => {
                match tag.as_str() {
                    "laster" => Some("latest".to_string()),
                    "stabl" | "stabel" => Some("stable".to_string()),
                    "betta" => Some("beta".to_string()),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Parser chính: parse `<name>[@<ref>]`
pub fn parse_scaffold_spec(core: CoreKind, input: &str) -> Result<ScaffoldSpec> {
    let (name, ref_str) = if let Some(pos) = input.find('@') {
        let (n, r) = input.split_at(pos);
        (n, Some(&r[1..])) // skip '@'
    } else {
        (input, None)
    };

    if name.is_empty() {
        bail!("Template name cannot be empty");
    }

    let requested_ref = match ref_str {
        None => ScaffoldRef::Default,
        Some(r) if r.is_empty() => ScaffoldRef::Default,
        Some(r) => {
            // Heuristic: nếu bắt đầu bằng số → version, không thì tag
            if r.chars().next().unwrap_or('a').is_ascii_digit() {
                ScaffoldRef::Version(r.to_string())
            } else if r.starts_with('^') || r.starts_with('~') {
                ScaffoldRef::Range(r.to_string())
            } else {
                ScaffoldRef::DistTag(r.to_string())
            }
        }
    };

    // Validate sớm
    requested_ref.validate()?;

    // Normalize name: kebab-case, lowercase
    let normalized_name = normalize_template_name(name);

    Ok(ScaffoldSpec {
        core,
        name: name.to_string(),
        normalized_name,
        requested_ref,
    })
}

/// Normalize template name: lowercase, kebab-case
fn normalize_template_name(name: &str) -> String {
    name.to_lowercase()
        .replace('_', "-")
        .trim()
        .to_string()
}

/// Artifact package naming: `mgc-create-<core>-<name>`
pub fn artifact_name(core: CoreKind, template_name: &str) -> String {
    format!("mgc-create-{}-{}", core.as_str(), template_name)
}
