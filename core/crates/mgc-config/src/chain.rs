//! Registry chain hợp nhất (ITEM 2): một nguồn sự thật cho thứ tự registry
//! — env override → mgc.toml [[registries]] (priority) → .npmrc registry= → npmjs.

use crate::registry::Registry;

pub const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";
pub const WEB_REGISTRY_URL_ENV: &str = "MAGICORE_WEB_REGISTRY_URL";

/// Chain registry theo thứ tự ưu tiên (entry 0 = primary).
/// env MAGICORE_WEB_REGISTRY_URL set → CHỈ registry đó (override — không thêm
/// fallback, giữ semantic cũ: test/CI pin 1 registry).
/// Ngược lại: mgc.toml [[registries]] (priority asc) → .npmrc registry= → npmjs
/// (dedupe theo URL).
pub fn registry_chain(
    project_root: Option<&std::path::Path>,
    project_config: Option<&crate::project::ProjectConfig>,
) -> Vec<Registry> {
    if let Some(env_url) = std::env::var(WEB_REGISTRY_URL_ENV)
        .ok()
        .filter(|v| !v.is_empty())
    {
        let mut r = Registry::new("env".to_string(), env_url);
        r.priority = 0;
        r.token = std::env::var("MAGICORE_WEB_REGISTRY_TOKEN").ok();
        return vec![r];
    }

    let mut chain: Vec<Registry> = Vec::new();

    if let Some(cfg) = project_config {
        let mut from_toml = cfg.registries.clone();
        from_toml.sort_by_key(|r| r.priority);
        for reg in from_toml {
            if !chain.iter().any(|c| same_url(&c.url, &reg.url)) {
                chain.push(reg);
            }
        }
    }

    if let Some(root) = project_root {
        if let Ok(npmrc) = crate::npmrc::NpmRc::load(root) {
            if let Some(url) = npmrc.registry_for(None) {
                if !chain.iter().any(|c| same_url(&c.url, &url)) {
                    let mut r = Registry::new("npmrc".to_string(), url.clone());
                    r.priority = u32::MAX;
                    r.token = npmrc.token_for(&host_of(&url)).cloned();
                    chain.push(r);
                }
            }
        }
    }

    if !chain.iter().any(|c| same_url(&c.url, DEFAULT_NPM_REGISTRY)) {
        let mut r = Registry::new("npmjs".to_string(), DEFAULT_NPM_REGISTRY.to_string());
        r.priority = u32::MAX;
        chain.push(r);
    }

    chain
}

fn same_url(a: &str, b: &str) -> bool {
    a.trim_end_matches('/')
        .eq_ignore_ascii_case(b.trim_end_matches('/'))
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.host_str().unwrap_or("").to_string())
        .unwrap_or_default()
}

