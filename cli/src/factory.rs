use mg_types::adapter::PackageAdapter;
/// Adapter factory — creates the right adapter based on ecosystem + feature flags.
/// Each core has a feature gate so single-core builds don't link unused crates.
use mg_types::Ecosystem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildShape {
    SingleCore,
    MultiCore,
}

/// Available cores in this build (for init menu filtering)
pub fn available_cores() -> Vec<(&'static str, &'static str)> {
    let mut cores: Vec<(&'static str, &'static str)> = Vec::new();
    macro_rules! push_core {
        ($feature:expr, $short:expr, $label:expr) => {
            if cfg!(feature = $feature) {
                cores.push(($short, $label));
            }
        };
    }
    push_core!("web", "web", "🌐  Web application");
    push_core!("game", "game", "🎮  Game");
    push_core!("ai", "ai", "🤖  AI agent / ML project");
    push_core!("clo", "clo", "☁️   Cloud infrastructure");
    push_core!("cicd", "cicd", "🔄  CI/CD pipeline");
    push_core!("iot", "iot", "🔌  IoT / Embedded device");
    push_core!("app", "app", "📱  Mobile / Desktop app");
    push_core!("lib", "lib", "📦  Library");
    cores
}

pub fn available_core_names() -> Vec<&'static str> {
    available_cores()
        .into_iter()
        .map(|(short, _)| short)
        .collect()
}

pub fn build_shape() -> BuildShape {
    if available_cores().len() == 1 {
        BuildShape::SingleCore
    } else {
        BuildShape::MultiCore
    }
}

pub fn is_single_core_build() -> bool {
    matches!(build_shape(), BuildShape::SingleCore)
}

/// Create an adapter for the given ecosystem.
/// Returns an error if the core is not available in this build.
pub fn create_adapter(ecosystem: &Ecosystem) -> anyhow::Result<Box<dyn PackageAdapter>> {
    match ecosystem {
        Ecosystem::Web => create_web_adapter(),
        Ecosystem::Game => create_game_adapter(),
        Ecosystem::Ai => create_ai_adapter(),
        Ecosystem::Cloud => create_clo_adapter(),
        Ecosystem::Cicd => create_cicd_adapter(),
        Ecosystem::Iot => create_iot_adapter(),
        Ecosystem::App => create_app_adapter(),
        Ecosystem::Lib => create_lib_adapter(),
    }
}

// ─── Per-core constructors ──────────────────────────────────────────

#[cfg(feature = "web")]
fn create_web_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    Ok(Box::new(mg_web_adapter::WebAdapter::new()))
}
#[cfg(not(feature = "web"))]
fn create_web_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!("Web core not available in this build. Install: brew install megagate (full) or megagate-web")
}

#[cfg(feature = "game")]
fn create_game_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'game' core is under development. Only the 'web' core is available in this release."
    )
}
#[cfg(not(feature = "game"))]
fn create_game_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'game' core is under development. Only the 'web' core is available in this release."
    )
}

#[cfg(feature = "ai")]
fn create_ai_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'ai' core is under development. Only the 'web' core is available in this release."
    )
}
#[cfg(not(feature = "ai"))]
fn create_ai_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'ai' core is under development. Only the 'web' core is available in this release."
    )
}

#[cfg(feature = "clo")]
fn create_clo_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'cloud' core is under development. Only the 'web' core is available in this release."
    )
}
#[cfg(not(feature = "clo"))]
fn create_clo_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'cloud' core is under development. Only the 'web' core is available in this release."
    )
}

#[cfg(feature = "cicd")]
fn create_cicd_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'cicd' core is under development. Only the 'web' core is available in this release."
    )
}
#[cfg(not(feature = "cicd"))]
fn create_cicd_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'cicd' core is under development. Only the 'web' core is available in this release."
    )
}

#[cfg(feature = "iot")]
fn create_iot_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'iot' core is under development. Only the 'web' core is available in this release."
    )
}
#[cfg(not(feature = "iot"))]
fn create_iot_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'iot' core is under development. Only the 'web' core is available in this release."
    )
}

#[cfg(feature = "app")]
fn create_app_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'app' core is under development. Only the 'web' core is available in this release."
    )
}
#[cfg(not(feature = "app"))]
fn create_app_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'app' core is under development. Only the 'web' core is available in this release."
    )
}

#[cfg(feature = "lib")]
fn create_lib_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'lib' core is under development. Only the 'web' core is available in this release."
    )
}
#[cfg(not(feature = "lib"))]
fn create_lib_adapter() -> anyhow::Result<Box<dyn PackageAdapter>> {
    anyhow::bail!(
        "'lib' core is under development. Only the 'web' core is available in this release."
    )
}
