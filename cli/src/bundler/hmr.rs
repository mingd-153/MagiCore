use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, trace};

/// HMR Event sent to the client
#[derive(Debug, Clone)]
pub enum HmrEvent {
    Reload,
    Update(String), // Path of the updated file
}

use std::sync::atomic::{AtomicU64, Ordering};

pub struct HmrManager {
    tx: broadcast::Sender<HmrEvent>,
    version: Arc<AtomicU64>,
}

impl HmrManager {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            tx,
            version: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HmrEvent> {
        self.tx.subscribe()
    }

    /// Watch a directory for changes and broadcast events
    pub fn watch_dir(&self, path: &Path) -> anyhow::Result<RecommendedWatcher> {
        let tx = self.tx.clone();
        let version = self.version.clone();

        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<Event>| match res {
                Ok(event) => {
                    trace!("HMR File Event: {:?}", event);
                    if event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove() {
                        version.fetch_add(1, Ordering::SeqCst);
                        let _ = tx.send(HmrEvent::Reload);
                    }
                }
                Err(e) => error!("watch error: {:?}", e),
            })?;

        watcher.watch(path, RecursiveMode::Recursive)?;
        info!("HMR watching directory: {}", path.display());
        Ok(watcher)
    }
}

pub async fn hmr_ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(manager): axum::extract::State<Arc<HmrManager>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, manager))
}

async fn handle_socket(mut socket: WebSocket, manager: Arc<HmrManager>) {
    let mut rx = manager.subscribe();

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(HmrEvent::Reload) => {
                        let msg = serde_json::json!({ "type": "reload" }).to_string();
                        if socket.send(Message::Text(msg)).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                    Ok(HmrEvent::Update(path)) => {
                        let msg = serde_json::json!({ "type": "update", "path": path }).to_string();
                        if socket.send(Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break, // Channel closed/lagged
                }
            }
            msg = socket.recv() => {
                if let Some(Ok(Message::Close(_))) = msg {
                    break;
                }
                if msg.is_none() {
                    break;
                }
            }
        }
    }
}

pub const HMR_CLIENT_SCRIPT: &str = r#"
(function() {
    console.log('[MgDevServer] Connecting to HMR WebSocket...');
    const ws = new WebSocket(`ws://${window.location.host}/@magicore/hmr`);
    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        if (data.type === 'reload') {
            console.log('[MgDevServer] File changed, reloading...');
            window.location.reload();
        }
    };
    ws.onclose = () => {
        console.warn('[MgDevServer] HMR connection lost. Server might be restarting.');
        setTimeout(() => window.location.reload(), 2000);
    };
})();
"#;
