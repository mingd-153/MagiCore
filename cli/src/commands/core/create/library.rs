//! `mgc create-lib` — scaffold library projects (Phase 2 all-core parity) — tạo project thư viện (Phase 2 cân bằng 4 cores)
//! Library project scaffolding with all-core parity (web/ai/app/lib uniform CLI surface).

use anyhow::Result;

pub async fn run(framework: &str, project_name: &str) -> Result<()> {
    // Parse scaffold spec sớm với typo detection (Phase 2 all-core parity) — parse sớm có typo detection (Phase 2 cân bằng cores)
    use crate::scaffold::spec::{parse_scaffold_spec, CoreKind};

    let parsed_framework = if !framework.is_empty() {
        Some(parse_scaffold_spec(CoreKind::Lib, framework).map_err(|e| {
            anyhow::anyhow!("Invalid framework specification '{}': {}", framework, e)
        })?)
    } else {
        None
    };

    // Config từ CLI, không dùng wizard — all-core parity với web/ai/app — config từ CLI (không wizard), parity với web/ai/app
    let lang = parsed_framework
        .as_ref()
        .map(|s| s.normalized_name.as_str())
        .unwrap_or("rust"); // default fallback — mặc định rust

    let config = crate::wizard::engine::ScaffoldConfig {
        project_name: project_name.to_string(),
        frameworks: vec![lang.to_string()],
        core: "lib".to_string(),
        sub_type: String::new(),
        features: vec![],
        template_dir: std::path::PathBuf::new(),
    };

    // Resolve layer: embedded → cache → registry → fallback — giải quyết layer: embedded → cache → registry → fallback
    match crate::commands::template::ensure_layer(&format!("lib/{}", lang)).await {
        Ok(status) if status.is_available() => {
            // Layer available - proceed — layer có sẵn - tiếp tục
        }
        Ok(_) | Err(_) => {
            // Fallback: generate warning, continue with minimal scaffold — fallback: cảnh báo, tiếp với scaffold tối thiểu
            mgc_ui::warning(&format!(
                "Optional lib layer 'lib/{}' is unavailable, using fallback",
                lang
            ));
        }
    }

    super::scaffold_and_save_metadata(&config)?;
    mgc_ui::success(&format!("Created lib project: {}", project_name));
    mgc_ui::info("Library project created. Next: `mgc add-lib` or `mgc install`.");
    Ok(())
}
