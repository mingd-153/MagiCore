//! mgc network verify — list all outbound connections + reachability (18 §4)
//! (Minh bạch mã nguồn mở: user tự kiểm tra MỌI kết nối đi ra.
//!  Chỉ TCP connect tới host — KHÔNG request thật, không gửi dữ liệu.)

use anyhow::Result;
use clap::Subcommand;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Subcommand, Debug, Clone)]
pub enum NetworkCmd {
    /// List all outbound connections (registry hosts + purposes) + reachability
    Verify {
        #[arg(short, long, help = "output JSON")]
        json: bool,
    },
}

pub struct Connection {
    pub host: String,
    pub port: u16,
    pub purpose: &'static str,
}

/// Nguồn chân lý của MỌI kết nối đi ra (không hardcode rải rác — RULE §12).
/// Thêm host mới → thêm vô đây + test.
pub fn outbound_connections() -> Vec<Connection> {
    vec![
        Connection {
            host: "registry.npmjs.org".into(),
            port: 443,
            purpose: "npm registry (install/fetch packages)",
        },
        Connection {
            host: "huggingface.co".into(),
            port: 443,
            purpose: "AI model hub (mgc model pull hf://)",
        },
        Connection {
            host: "github.com".into(),
            port: 443,
            purpose: "GitHub releases / repos (self-update, templates)",
        },
        Connection {
            host: "pypi.org".into(),
            port: 443,
            purpose: "PyPI (python packages proxy)",
        },
        // Registries user-config trong mgc.toml / ~/.config/magicore
    ]
}

/// Reachability: TCP connect timeout 3s — không gửi dữ liệu, không request.
fn reachable(host: &str, port: u16) -> bool {
    let addr = format!("{host}:{port}");
    match addr.to_socket_addrs() {
        Ok(mut addrs) => addrs
            .find_map(|a| {
                TcpStream::connect_timeout(&a, Duration::from_secs(3))
                    .ok()
                    .map(|_| true)
            })
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Pub cho mgc doctor dùng (T7b) — passthrough tới `reachable`
pub fn reachable_public(host: &str, port: u16) -> bool {
    reachable(host, port)
}

/// Gộp host registry user-configured (mgc.toml [registry]) — đọc an toàn, thiếu file → bỏ qua.
fn user_registries() -> Vec<Connection> {
    let mut out = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        let mgc_toml = cwd.join("mgc.toml");
        if let Ok(raw) = std::fs::read_to_string(&mgc_toml) {
            let cfg: Option<toml::Value> = toml::from_str(&raw).ok();
            if let Some(cfg) = cfg {
                if let Some(regs) = cfg.get("registry") {
                    let urls: Vec<String> = match regs {
                        toml::Value::Table(t) => t
                            .values()
                            .filter_map(|v| {
                                v.get("url").and_then(|u| u.as_str()).map(str::to_string)
                            })
                            .collect(),
                        toml::Value::String(s) => vec![s.clone()],
                        _ => vec![],
                    };
                    for url in urls {
                        if let Some(host) = url_host(&url) {
                            out.push(Connection {
                                host: host.0,
                                port: host.1,
                                purpose: "user-configured registry (mgc.toml)",
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

fn url_host(url: &str) -> Option<(String, u16)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host_port: Vec<&str> = rest.split('/').next()?.split(':').collect();
    let port = host_port
        .get(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(if url.starts_with("https://") { 443 } else { 80 });
    Some((host_port[0].to_string(), port))
}

pub fn handle(cmd: NetworkCmd) -> Result<()> {
    let mut all: Vec<Connection> = outbound_connections();
    all.extend(user_registries());

    #[derive(serde::Serialize)]
    struct Row<'a> {
        host: &'a str,
        port: u16,
        purpose: &'a str,
        reachable: bool,
    }
    let rows: Vec<Row> = all
        .iter()
        .map(|c| Row {
            host: &c.host,
            port: c.port,
            purpose: c.purpose,
            reachable: reachable(&c.host, c.port),
        })
        .collect();

    let json = match &cmd {
        NetworkCmd::Verify { json } => *json,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for r in &rows {
            let mark = if r.reachable { "OK" } else { "UNREACHABLE" };
            println!("{mark:12} {}:{}  ({})", r.host, r.port, r.purpose);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../test/network_test.rs"]
mod tests;
