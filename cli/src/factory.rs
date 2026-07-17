use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;

/// Available cores in this build (for init menu filtering)
pub fn available_cores() -> Vec<(&'static str, &'static str)> {
    vec![
        ("web", "🌐  Web application"),
        ("game", "🎮  Game"),
        ("ai", "🤖  AI agent / ML project"),
        ("clo", "☁️   Cloud infrastructure"),
        ("cicd", "🔄  CI/CD pipeline"),
        ("iot", "🔌  IoT / Embedded device"),
        ("app", "📱  Mobile / Desktop app"),
        ("lib", "📦  Library"),
    ]
}

/// Create an adapter for the given ecosystem.
pub fn create_adapter(ecosystem: &Ecosystem) -> anyhow::Result<Box<dyn PackageAdapter>> {
    match ecosystem {
        Ecosystem::Web => Ok(Box::new(mg_web_adapter::WebAdapter::new())),
        Ecosystem::Game => anyhow::bail!("'game' core is under development."),
        Ecosystem::Ai => anyhow::bail!("'ai' core is under development."),
        Ecosystem::Cloud => anyhow::bail!("'cloud' core is under development."),
        Ecosystem::Cicd => anyhow::bail!("'cicd' core is under development."),
        Ecosystem::Iot => anyhow::bail!("'iot' core is under development."),
        Ecosystem::App => anyhow::bail!("'app' core is under development."),
        Ecosystem::Lib => anyhow::bail!("'lib' core is under development."),
    }
}
