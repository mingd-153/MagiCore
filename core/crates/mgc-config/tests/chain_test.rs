use mgc_config::chain::*;
use mgc_config::registry::Registry;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[test]
fn chain_env_overrides_everything() {
    let _guard = env_lock();
    std::env::set_var(WEB_REGISTRY_URL_ENV, "http://127.0.0.1:9");
    let chain = registry_chain(None, None);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].url, "http://127.0.0.1:9");
    std::env::remove_var(WEB_REGISTRY_URL_ENV);
}

#[test]
fn chain_defaults_to_npmjs_without_config() {
    let _guard = env_lock();
    std::env::remove_var(WEB_REGISTRY_URL_ENV);
    let chain = registry_chain(None, None);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].url, DEFAULT_NPM_REGISTRY);
}

#[test]
fn chain_dedupes_same_url() {
    let _guard = env_lock();
    std::env::remove_var(WEB_REGISTRY_URL_ENV);
    let mut cfg = mgc_config::project::ProjectConfig::new("x", "web");
    cfg.registries.push(Registry::new(
        "a".into(),
        "https://registry.npmjs.org/".into(),
    ));
    let chain = registry_chain(None, Some(&cfg));
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].url, "https://registry.npmjs.org/");
}
