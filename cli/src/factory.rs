use std::sync::Arc;

use mg_plugin::Plugin;
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;

/// Available cores in this build (for init menu filtering)
pub fn available_cores() -> Vec<(&'static str, &'static str)> {
    let mut cores = Vec::new();
    #[cfg(feature = "web")]
    cores.push(("web", "🌐  Web application"));
    #[cfg(feature = "lib")]
    cores.push(("lib", "📚  Library (ts / rust / python)"));
    #[cfg(feature = "game")]
    cores.push(("game", "🎮  Game (bevy / godot / unity / unreal)"));
    #[cfg(feature = "iot")]
    cores.push(("iot", "📡  IoT (esp32-rust / platformio / zephyr)"));
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
/// ponytail: 1 tiến trình cli = 1 registry config cố định (mg.toml/env) — bỏ
/// qua so khớp url/token khi reuse; nếu sau này chạy multi-config trong 1
/// tiến trình thì thêm check.
pub fn create_adapter(
    ecosystem: &Ecosystem,
    registry_url: Option<&str>,
    token: Option<&str>,
) -> anyhow::Result<Arc<dyn PackageAdapter>> {
    if let Some(plugin) = mg_plugin::global().get(*ecosystem) {
        if let Some(adapter) = plugin.as_adapter() {
            return Ok(adapter);
        }
    }

    let adapter: Arc<dyn PackageAdapter> = match ecosystem {
        #[cfg(feature = "web")]
        Ecosystem::Web => Arc::new(match (registry_url, token) {
            (Some(url), _) => mg_web_adapter::WebAdapter::with_registry_and_token(
                url.to_string(),
                token.map(str::to_string),
            ),
            _ => mg_web_adapter::WebAdapter::new(),
        }),
        #[cfg(not(feature = "web"))]
        Ecosystem::Web => anyhow::bail!("'web' core is not included in this build."),
        #[cfg(feature = "game")]
        Ecosystem::Game => Arc::new(
            mg_game_adapter::adapter_for(&std::env::current_dir()?)
                .ok_or_else(|| anyhow::anyhow!("Cannot detect a game project here (missing mg.toml/project.godot/manifest.json/.uproject)."))?,
        ),
        #[cfg(not(feature = "game"))]
        Ecosystem::Game => anyhow::bail!("'game' core is not included in this build."),
        Ecosystem::Ai => anyhow::bail!("'ai' core is under development."),
        #[cfg(feature = "clo")]
        Ecosystem::Cloud => Arc::new(
            mg_cloud_adapter::adapter_for(&std::env::current_dir()?)
                .ok_or_else(|| anyhow::anyhow!("Cannot detect a cloud project here (missing mg.toml/Pulumi.yaml/*.tf/cdk package.json)."))?,
        ),
        #[cfg(not(feature = "clo"))]
        Ecosystem::Cloud => anyhow::bail!("'cloud' core is not included in this build."),
        Ecosystem::Cicd => anyhow::bail!("'cicd' core is under development."),
        #[cfg(feature = "iot")]
        Ecosystem::Iot => Arc::new(
            mg_iot_adapter::adapter_for(&std::env::current_dir()?)
                .ok_or_else(|| anyhow::anyhow!("Cannot detect an iot project here (missing mg.toml/platformio.ini/west.yml)."))?,
        ),
        #[cfg(not(feature = "iot"))]
        Ecosystem::Iot => anyhow::bail!("'iot' core is not included in this build."),
        Ecosystem::App => anyhow::bail!("'app' core is under development."),
        #[cfg(feature = "hardware")]
        Ecosystem::Hardware => Arc::new(
            mg_hardware_adapter::adapter_for(&std::env::current_dir()?)
                .ok_or_else(|| anyhow::anyhow!("Cannot detect a hardware project here (missing mg.toml with ecosystem = \"hardware\")."))?,
        ),
        #[cfg(not(feature = "hardware"))]
        Ecosystem::Hardware => anyhow::bail!("'hardware' core is not included in this build."),
        #[cfg(feature = "lib")]
        Ecosystem::Lib => Arc::new(
            mg_lib_adapter::adapter_for(
                &std::env::current_dir()?,
                registry_url.map(str::to_string),
                token.map(str::to_string),
            )
            .ok_or_else(|| anyhow::anyhow!("Cannot detect a lib project here (missing mg.toml/lib marker)."))?,
        ),
        #[cfg(not(feature = "lib"))]
        Ecosystem::Lib => anyhow::bail!("'lib' core is not included in this build."),
    };

    // Đăng ký plugin vào registry global — lần gọi sau dispatch qua registry.
    let _ = mg_plugin::register(Plugin::from_adapter(adapter.clone()));
    Ok(adapter)
}
