use std::sync::Arc;

use mgc_plugin::Plugin;
use mgc_types::adapter::PackageAdapter;
use mgc_types::Ecosystem;

/// Available cores in this build (for init menu filtering)
#[allow(clippy::vec_init_then_push)]
pub fn available_cores() -> Vec<(&'static str, &'static str)> {
    let mut cores = Vec::new();
    #[cfg(feature = "web")]
    cores.push(("web", "🌐  Web application"));
    #[cfg(feature = "lib")]
    cores.push(("lib", "📚  Library (ts / rust / python)"));
    #[cfg(feature = "ai")]
    cores.push(("ai", "🤖  AI agent / model runtime"));
    #[cfg(feature = "app")]
    cores.push(("app", "📱  App (flutter / kotlin / swift)"));
    #[cfg(feature = "game")]
    cores.push(("game", "🎮  Game (bevy / godot / unity / unreal)"));
    #[cfg(feature = "iot")]
    cores.push(("iot", "📡  IoT (esp32-rust / platformio / zephyr)"));
    #[cfg(feature = "clo")]
    cores.push(("clo", "☁️  Cloud infrastructure"));
    #[cfg(feature = "cicd")]
    cores.push(("cicd", "🚦  CI/CD pipeline"));
    #[cfg(feature = "hardware")]
    cores.push((
        "hardware",
        "⚙️  Hardware (optimizer/bench — GPU/CPU acceleration)",
    ));
    cores
}

/// Create an adapter for the given ecosystem — dispatch qua PluginRegistry
/// (T3): registry có plugin → dùng plugin (adapter back-ref); miss → tạo như
/// cũ + đăng ký global (lần sau dùng registry).
///
/// ponytail: 1 tiến trình cli = 1 registry config cố định (mgc.toml/env) — bỏ
/// qua so khớp url/token khi reuse; nếu sau này chạy multi-config trong 1
/// tiến trình thì thêm check.
pub fn create_adapter(
    ecosystem: &Ecosystem,
    registry_url: Option<&str>,
    token: Option<&str>,
) -> anyhow::Result<Arc<dyn PackageAdapter>> {
    create_adapter_for(
        &std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?,
        ecosystem,
        registry_url,
        token,
        &[],
    )
}

/// Tạo adapter gắn với project root rõ ràng (mix core: workspace target).
/// `fallbacks`: (url, token) chain — chỉ dùng khi primary 404/network/5xx.
pub fn create_adapter_for(
    root: &std::path::Path,
    ecosystem: &Ecosystem,
    registry_url: Option<&str>,
    token: Option<&str>,
    fallbacks: &[(String, Option<String>)],
) -> anyhow::Result<Arc<dyn PackageAdapter>> {
    let _ = root;
    if let Some(plugin) = mgc_plugin::global().get(*ecosystem) {
        if let Some(adapter) = plugin.as_adapter() {
            return Ok(adapter);
        }
    }

    let adapter: Arc<dyn PackageAdapter> = match ecosystem {
        #[cfg(feature = "web")]
        Ecosystem::Web => Arc::new(match (registry_url, token) {
            (Some(url), _) => mgc_web_adapter::WebAdapter::with_registry_chain(
                url.to_string(),
                token.map(str::to_string),
                fallbacks.to_vec(),
            ),
            _ => mgc_web_adapter::WebAdapter::new(),
        }),
        #[cfg(not(feature = "web"))]
        Ecosystem::Web => return Err(crate::error::core_not_in_build("web")),
        #[cfg(feature = "game")]
        Ecosystem::Game => Arc::new(
            mgc_game_adapter::adapter_for(root)
                .ok_or_else(|| crate::error::detect_core_failed("game"))?,
        ),
        #[cfg(not(feature = "game"))]
        Ecosystem::Game => return Err(crate::error::core_not_in_build("game")),
        #[cfg(feature = "ai")]
        Ecosystem::Ai => Arc::new(
            mgc_ai_adapter::adapter_for(root)
                .ok_or_else(|| crate::error::detect_core_failed("ai"))?,
        ),
        #[cfg(not(feature = "ai"))]
        Ecosystem::Ai => return Err(crate::error::core_not_in_build("ai")),
        #[cfg(feature = "clo")]
        Ecosystem::Cloud => Arc::new(
            mgc_cloud_adapter::adapter_for(root)
                .ok_or_else(|| crate::error::detect_core_failed("clo"))?,
        ),
        #[cfg(not(feature = "clo"))]
        Ecosystem::Cloud => return Err(crate::error::core_not_in_build("clo")),
        #[cfg(feature = "cicd")]
        Ecosystem::Cicd => Arc::new(
            mgc_cicd_adapter::adapter_for(root)
                .ok_or_else(|| crate::error::detect_core_failed("cicd"))?,
        ),
        #[cfg(not(feature = "cicd"))]
        Ecosystem::Cicd => return Err(crate::error::core_not_in_build("cicd")),
        #[cfg(feature = "iot")]
        Ecosystem::Iot => Arc::new(
            mgc_iot_adapter::adapter_for(root)
                .ok_or_else(|| crate::error::detect_core_failed("iot"))?,
        ),
        #[cfg(not(feature = "iot"))]
        Ecosystem::Iot => return Err(crate::error::core_not_in_build("iot")),
        #[cfg(feature = "app")]
        Ecosystem::App => Arc::new(
            mgc_app_adapter::adapter_for(root)
                .ok_or_else(|| crate::error::detect_core_failed("app"))?,
        ),
        #[cfg(not(feature = "app"))]
        Ecosystem::App => return Err(crate::error::core_not_in_build("app")),
        #[cfg(feature = "hardware")]
        Ecosystem::Hardware => Arc::new(
            mgc_hardware_adapter::adapter_for(root)
                .ok_or_else(|| crate::error::detect_core_failed("hardware"))?,
        ),
        #[cfg(not(feature = "hardware"))]
        Ecosystem::Hardware => return Err(crate::error::core_not_in_build("hardware")),
        #[cfg(feature = "lib")]
        Ecosystem::Lib => Arc::new(
            mgc_lib_adapter::adapter_for_with_chain(
                root,
                registry_url.map(str::to_string),
                token.map(str::to_string),
                fallbacks,
            )
            .ok_or_else(|| crate::error::detect_core_failed("lib"))?,
        ),
        #[cfg(not(feature = "lib"))]
        Ecosystem::Lib => return Err(crate::error::core_not_in_build("lib")),
    };

    // Đăng ký plugin vào registry global — lần gọi sau dispatch qua registry.
    let _ = mgc_plugin::register(Plugin::from_adapter(adapter.clone()));
    Ok(adapter)
}
