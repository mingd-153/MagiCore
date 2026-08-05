use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;

/// Available cores in this build (for init menu filtering)
pub fn available_cores() -> Vec<(&'static str, &'static str)> {
    let mut cores = Vec::new();
    #[cfg(feature = "web")]
    cores.push(("web", "🌐  Web application"));
    cores
}

/// Create an adapter for the given ecosystem.
/// `registry_url`/`token`: optional registry override từ mg.toml [registry].
pub fn create_adapter(
    ecosystem: &Ecosystem,
    registry_url: Option<&str>,
    token: Option<&str>,
) -> anyhow::Result<Box<dyn PackageAdapter>> {
    match ecosystem {
        #[cfg(feature = "web")]
        Ecosystem::Web => Ok(Box::new(match (registry_url, token) {
            (Some(url), _) => mg_web_adapter::WebAdapter::with_registry_and_token(
                url.to_string(),
                token.map(str::to_string),
            ),
            _ => mg_web_adapter::WebAdapter::new(),
        })),
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
