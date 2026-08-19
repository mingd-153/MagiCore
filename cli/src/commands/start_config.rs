// Start bind configuration — validates local production server binding.
// Cấu hình bind cho `mg start` — giữ mặc định local-first và port đúng RULE.
use anyhow::{bail, Context, Result};

pub const DEFAULT_WEB_START_HOST: &str = "localhost";
pub const DEFAULT_WEB_START_PORT: u16 = 4315;
pub const WEB_START_HOST_ENV: &str = "MEGAGATE_WEB_START_HOST";
pub const WEB_START_PORT_ENV: &str = "MEGAGATE_WEB_START_PORT";
pub const VALID_RULE_PORTS: [u16; 24] = [
    4315, 4351, 4135, 4153, 4513, 4531, 3415, 3451, 3145, 3154, 3541, 3514, 1345, 1354, 1435, 1453,
    1534, 1543, 5134, 5143, 5314, 5341, 5413, 5431,
];

pub fn resolve_web_start_bind() -> Result<(String, u16)> {
    resolve_web_start_bind_from_env(
        std::env::var(WEB_START_HOST_ENV).ok().as_deref(),
        std::env::var(WEB_START_PORT_ENV).ok().as_deref(),
    )
}

pub fn resolve_web_start_bind_from_env(
    host_raw: Option<&str>,
    port_raw: Option<&str>,
) -> Result<(String, u16)> {
    let host = host_raw
        .unwrap_or(DEFAULT_WEB_START_HOST)
        .trim()
        .to_string();
    if host.is_empty() {
        bail!("{WEB_START_HOST_ENV} cannot be empty");
    }
    if host == "0.0.0.0" || host == "::" {
        bail!("{WEB_START_HOST_ENV} cannot bind all interfaces for local product start");
    }

    let port = match port_raw {
        Some(raw) => raw
            .parse::<u16>()
            .with_context(|| format!("{WEB_START_PORT_ENV} must be a TCP port"))?,
        None => DEFAULT_WEB_START_PORT,
    };
    if !VALID_RULE_PORTS.contains(&port) {
        bail!("{WEB_START_PORT_ENV} must be one of the approved MegaGate ports");
    }

    Ok((host, port))
}
