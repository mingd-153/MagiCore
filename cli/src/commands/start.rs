use anyhow::{bail, Result};
use axum::Router;
use colored::Colorize;
use mg_ui::{info, success};
use std::path::PathBuf;
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::context::ProjectContext;

/// mg start — MegaGate Native Production Server.
/// Detects output directory dynamically (dist, build, out) and serves it using Axum.
pub async fn run(core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let root = ctx.root();

    if !mg_ui::is_quiet() {
        mg_ui::blank_line();
        println!("🚀 {}", "MegaGate Production Server".bold().magenta());
    }
    info(&format!("Project root: {}", root.display()));

    match ctx.adapter().name() {
        "web" => {
            let fe_port = 4315u16;

            // Dynamically detect build output directory
            let possible_dirs = vec!["dist", "build", "out", ".next", "public"];
            let mut selected_dir: Option<PathBuf> = None;

            for dir in possible_dirs {
                let path = root.join(dir);
                if path.exists() && path.is_dir() {
                    selected_dir = Some(path);
                    break;
                }
            }

            let dist_dir = match selected_dir {
                Some(d) => d,
                None => bail!("Could not find a valid build output directory (checked: dist, build, out, .next, public). Please run 'mg build' first."),
            };

            info(&format!(
                "Serving directory '{}' → http://127.0.0.1:{}",
                dist_dir.file_name().unwrap_or_default().to_string_lossy(),
                fe_port
            ));

            // Serve static files from the detected directory
            // Fallback to `index.html` for SPA routing (React/Vue/Svelte)
            let serve_dir =
                ServeDir::new(&dist_dir).fallback(ServeFile::new(dist_dir.join("index.html")));

            let app = Router::new()
                .nest_service("/", serve_dir)
                // Add automatic compression (gzip/brotli/deflate)
                .layer(CompressionLayer::new());

            let bind_addr = format!("127.0.0.1:{}", fe_port);
            let listener = TcpListener::bind(&bind_addr).await?;

            success(&format!(
                "Production Server running natively on http://{}",
                bind_addr
            ));
            axum::serve(listener, app).await?;

            Ok(())
        }
        other => {
            bail!("'mg start' Engine is not implemented for the '{other}' core yet")
        }
    }
}
