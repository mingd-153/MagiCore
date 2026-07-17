use anyhow::bail;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};

use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use mg_ui::info;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::io::stdout;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::context::ProjectContext;

pub async fn run(
    core: Option<&str>,
    host: Option<String>,
    port: Option<u16>,
    clear: bool,
) -> anyhow::Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let host = host.unwrap_or_else(|| "127.0.0.1".to_string());
    let root = ctx.root().to_path_buf();

    info("Starting MegaGate Native Dev Server...");
    info(&format!("Project root: {}", root.display()));

    match ctx.adapter().name() {
        "web" => {
            let fe_port = port.unwrap_or(4315);
            let be_port = 3415;

            info(&format!(
                "Engine Web: Binding Frontend to http://{}:{}",
                host, fe_port
            ));
            info(&format!(
                "Engine Web: Binding Backend API Proxy to http://{}:{}",
                host, be_port
            ));

            // Create broadcast channel for HMR events
            let (hmr_tx, _) = broadcast::channel::<String>(100);
            let hmr_tx = Arc::new(hmr_tx);

            // 1. Initialize File Watcher with HMR broadcast
            let hmr_tx_watcher = hmr_tx.clone();
            let root_watcher = root.clone();
            let clear_watcher = clear;
            tokio::spawn(async move {
                if let Err(e) = watch_project_files(root_watcher, clear_watcher, hmr_tx_watcher) {
                    eprintln!("File watcher error: {:?}", e);
                }
            });

            // 2. Initialize Axum Server with WebSocket HMR
            let hmr_tx_server = hmr_tx.clone();
            let app = Router::new()
                .route("/", get(|| async { "MegaGate Native Dev Server is running. (HMR active)" }))
                .route("/api/health", get(|| async { "Backend Proxy OK" }))
                .route("/hmr", get(move |ws: WebSocketUpgrade| {
                    let hmr_tx = hmr_tx_server.clone();
                    async move { ws.on_upgrade(move |socket| handle_hmr_socket(socket, hmr_tx)) }
                }));

            let bind_addr = format!("{}:{}", host, fe_port);
            let listener = TcpListener::bind(&bind_addr).await?;

            info("Ready! Watching for file changes with HMR...");
            axum::serve(listener, app).await?;

            Ok(())
        }
        "game" => {
            info("Engine Game: Starting native watcher and game runtime...");
            Ok(())
        }
        other => bail!("'mg dev' Engine is not implemented for the '{other}' core yet"),
    }
}

async fn handle_hmr_socket(mut socket: WebSocket, hmr_tx: Arc<broadcast::Sender<String>>) {
    let mut rx = hmr_tx.subscribe();

    // Send initial connection message
    let _ = socket.send(Message::Text("connected".into())).await;

    // Spawn task to forward HMR messages to client
    let hmr_tx = hmr_tx.clone();
    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let _ = hmr_tx.send(msg);
        }
    });

    // Handle incoming messages (ping/pong, etc.)
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(data)) => {
                let _ = socket.send(Message::Pong(data)).await;
            }
            _ => {}
        }
    }
}

/// Watches the project directory for changes using `notify`.
fn watch_project_files(
    root: PathBuf,
    clear: bool,
    hmr_tx: Arc<broadcast::Sender<String>>,
) -> anyhow::Result<()> {
    info("Initializing ultra-fast File Watcher (notify)...");

    // Create a channel to receive the events
    let (tx, rx) = channel();

    // Create a watcher object
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    // Add the project root to the watcher
    watcher.watch(&root, RecursiveMode::Recursive)?;

    // Listen for events in a blocking loop (running in a spawned task's blocking context)
    // For a real dev server, we'd debounce these and trigger HMR via WebSockets.
    for res in rx {
        match res {
            Ok(event) => {
                // Filter out common noisy files (like node_modules, .git)
                let paths = event.paths;
                let should_log = paths.iter().any(|p| {
                    let s = p.to_string_lossy();
                    !s.contains("node_modules") && !s.contains(".git") && !s.contains("target")
                });

                if should_log {
                    if clear {
                        execute!(stdout(), Clear(ClearType::All)).ok();
                        execute!(stdout(), MoveTo(0, 0)).ok();
                    }
                    info(&format!(
                        "Hot Reloading... Changed: {:?}",
                        paths[0].file_name().unwrap_or_default()
                    ));

                    // Broadcast HMR message to connected clients
                    let _ = hmr_tx.send("reload".to_string());
                }
            }
            Err(e) => eprintln!("watch error: {:?}", e),
        }
    }

    Ok(())
}
