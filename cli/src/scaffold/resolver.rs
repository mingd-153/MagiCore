//! Scaffold resolver — typed resolution thay vì bool
//! Resolver với kết quả rõ ràng: embedded/cache hit/fetched/missing

use std::fmt;
use std::path::PathBuf;

use crate::scaffold::spec::{CoreKind, ScaffoldSpec};

/// Kết quả resolve scaffold - typed thay vì bool
#[derive(Debug, Clone)]
pub enum ScaffoldResolveStatus {
    /// Có trong embedded kernel
    Embedded { layer: String },
    /// Cache hit (đã fetch trước)
    CacheHit {
        layer: String,
        version: Option<String>,
        path: PathBuf,
    },
    /// Vừa fetch từ registry
    Fetched {
        layer: String,
        version: String,
        path: PathBuf,
    },
    /// Missing nhưng optional (không block)
    OptionalMissing { layer: String, reason: String },
}

impl ScaffoldResolveStatus {
    /// Check if resolved successfully (embedded/cache/fetched)
    pub fn is_available(&self) -> bool {
        matches!(
            self,
            ScaffoldResolveStatus::Embedded { .. }
                | ScaffoldResolveStatus::CacheHit { .. }
                | ScaffoldResolveStatus::Fetched { .. }
        )
    }

    /// Get layer name
    pub fn layer(&self) -> &str {
        match self {
            ScaffoldResolveStatus::Embedded { layer }
            | ScaffoldResolveStatus::CacheHit { layer, .. }
            | ScaffoldResolveStatus::Fetched { layer, .. }
            | ScaffoldResolveStatus::OptionalMissing { layer, .. } => layer,
        }
    }
}

/// Lỗi resolve scaffold - typed với suggestions
#[derive(Debug)]
pub enum ScaffoldResolveError {
    /// Tag không hợp lệ (typo)
    InvalidTag {
        template: String,
        input: String,
        suggestion: Option<String>,
    },

    /// Required layer thiếu
    RequiredLayerMissing {
        layer: String,
        core: String,
        template: String,
        tag: String,
        attempted_sources: String,
    },

    /// Registry unavailable
    RegistryUnavailable { registry: String, offline: bool },

    /// Integrity mismatch
    IntegrityMismatch {
        artifact: String,
        expected: String,
        actual: String,
    },

    /// Core không support
    UnsupportedCore { core: String },

    /// Template không support trong core
    UnsupportedTemplate { core: String, template: String },

    /// Generic error
    Other(String),
}

impl fmt::Display for ScaffoldResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScaffoldResolveError::InvalidTag {
                template,
                input,
                suggestion,
            } => {
                write!(f, "Unknown scaffold tag '{}' for '{}'", input, template)?;
                if let Some(s) = suggestion {
                    write!(f, ". Did you mean '{}'?", s)?;
                }
                Ok(())
            }
            ScaffoldResolveError::RequiredLayerMissing {
                layer,
                core,
                template,
                tag,
                attempted_sources,
            } => {
                write!(
                    f,
                    "Required scaffold layer '{}' is not available.\n\nTried:\n{}\n\nNext steps:\n- Run: mgc template fetch {} {}@{}\n- Or configure registry: mgc config set registry.scaffold <url>",
                    layer, attempted_sources, core, template, tag
                )
            }
            ScaffoldResolveError::RegistryUnavailable { registry, offline } => {
                write!(f, "Registry '{}' is unavailable", registry)?;
                if *offline {
                    write!(f, " (offline mode)")?;
                }
                Ok(())
            }
            ScaffoldResolveError::IntegrityMismatch {
                artifact,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Integrity check failed for '{}': expected {}, got {}",
                    artifact, expected, actual
                )
            }
            ScaffoldResolveError::UnsupportedCore { core } => {
                write!(
                    f,
                    "Unsupported core: '{}'. Supported: web, ai, app, lib",
                    core
                )
            }
            ScaffoldResolveError::UnsupportedTemplate { core, template } => {
                write!(f, "Unsupported template '{}' for core '{}'", template, core)
            }
            ScaffoldResolveError::Other(msg) => {
                write!(f, "Scaffold resolve failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for ScaffoldResolveError {}

/// Aggregated missing layers report (gom nhiều lỗi thành một)
#[derive(Debug, Default)]
pub struct MissingLayersReport {
    pub required: Vec<String>,
    pub optional: Vec<String>,
}

impl MissingLayersReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_required(&mut self, layer: String) {
        self.required.push(layer);
    }

    pub fn add_optional(&mut self, layer: String) {
        self.optional.push(layer);
    }

    pub fn has_required_missing(&self) -> bool {
        !self.required.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.required.is_empty() && self.optional.is_empty()
    }

    /// Format thành error message duy nhất thay vì spam warnings
    pub fn format_error(&self, core: &str, template: &str) -> String {
        let mut msg = String::new();

        if !self.required.is_empty() {
            msg.push_str(&format!(
                "Required scaffold layers missing ({} total):\n",
                self.required.len()
            ));
            for layer in &self.required {
                msg.push_str(&format!("  - {}\n", layer));
            }
            msg.push_str(&format!(
                "\nRun: mgc template fetch {} {}@latest\n",
                core, template
            ));
        }

        if !self.optional.is_empty() {
            if !msg.is_empty() {
                msg.push('\n');
            }
            msg.push_str(&format!(
                "Optional layers not found ({} total) - scaffold may have reduced features\n",
                self.optional.len()
            ));
        }

        msg
    }
}

/// Helper: convert spec to layer path
pub fn spec_to_layer_path(spec: &ScaffoldSpec) -> String {
    // Ví dụ: web/frontend/nextjs, ai/fastapi, app/flutter
    match spec.core {
        CoreKind::Web => format!("web/frontend/{}", spec.normalized_name),
        CoreKind::Ai => format!("ai/{}", spec.normalized_name),
        CoreKind::App => format!("app/{}", spec.normalized_name),
        CoreKind::Lib => format!("lib/{}", spec.normalized_name),
        CoreKind::Game => format!("game/{}", spec.normalized_name),
        CoreKind::Iot => format!("iot/{}", spec.normalized_name),
        CoreKind::Cicd => format!("cicd/{}", spec.normalized_name),
        CoreKind::Cloud => format!("cloud/{}", spec.normalized_name),
    }
}
