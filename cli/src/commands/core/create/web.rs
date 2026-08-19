//! `mg <verb> web` — re-export tách từ core/web.rs (Phase 7 v5).
//! Logic giữ nguyên tại core/web.rs (implementation duy nhất, không nhân bản).

pub use crate::commands::core::web::{run_create_with_options};

pub(crate) use crate::commands::core::web::{enrich_web_project_manifest, parse_framework_request};
