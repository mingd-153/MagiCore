use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;

/// Available cores in this build (for init menu filtering)
pub fn available_cores() -> Vec<(&'static str, &'static str)> {
    let mut cores = Vec::new();
    #[cfg(feature = "web")]
    cores.push(("web", "🌐  Web application"));
    #[cfg(feature = "game")]
    cores.push(("game", "🎮  Game"));
    #[cfg(feature = "ai")]
    cores.push(("ai", "🤖  AI agent / ML project"));
    #[cfg(feature = "clo")]
    cores.push(("clo", "☁️   Cloud infrastructure"));
    #[cfg(feature = "cicd")]
    cores.push(("cicd", "🔄  CI/CD pipeline"));
    #[cfg(feature = "iot")]
    cores.push(("iot", "🔌  IoT / Embedded device"));
    #[cfg(feature = "app")]
    cores.push(("app", "📱  Mobile / Desktop app"));
    #[cfg(feature = "lib")]
    cores.push(("lib", "📦  Library"));
    cores
}

/// Create an adapter for the given ecosystem.
pub fn create_adapter(ecosystem: &Ecosystem) -> anyhow::Result<Box<dyn PackageAdapter>> {
    match ecosystem {
        #[cfg(feature = "web")]
        Ecosystem::Web => Ok(Box::new(mg_web_adapter::WebAdapter::new())),
        #[cfg(not(feature = "web"))]
        Ecosystem::Web => anyhow::bail!("'web' core is not included in this build."),
        Ecosystem::Game => anyhow::bail!("'game' core is under development."),
        Ecosystem::Ai => anyhow::bail!("'ai' core is under development."),
        Ecosystem::Cloud => anyhow::bail!("'cloud' core is under development."),
        Ecosystem::Cicd => anyhow::bail!("'cicd' core is under development."),
        Ecosystem::Iot => anyhow::bail!("'iot' core is under development."),
        Ecosystem::App => anyhow::bail!("'app' core is under development."),
        Ecosystem::Lib => anyhow::bail!("'lib' core is under development."),
    }
}
