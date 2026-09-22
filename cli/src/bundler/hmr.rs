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

impl Default for HmrManager {
    fn default() -> Self {
        Self::new()
    }
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

    /// Watch a directory for changes and broadcast events.
    /// (Theo dõi thư mục thay đổi và phát sự kiện.)
    pub fn watch_dir(&self, path: &Path) -> anyhow::Result<RecommendedWatcher> {
        let tx = self.tx.clone();
        let version = self.version.clone();

        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<Event>| match res {
                Ok(event) => {
                    trace!("HMR File Event: {:?}", event);
                    if event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove() {
                        // Noise filter (P3, fresh-context review
                        // 2026-09-15): when the project has no src/ the
                        // watcher falls back to the ROOT — and a root
                        // watch fires on node_modules churn (installs,
                        // .magicore store writes) which nobody can fix by
                        // editing a file. Reload is only ever USEFUL
                        // for edits a human makes; churn under
                        // node_modules/.magicore/dist/build is machine
                        // noise. Filter at the EVENT level: every event
                        // path must NOT contain a noise component.
                        // (Lọc nhiễu (P3): khi project không có src/,
                        // watcher fallback về ROOT — và root watch kích
                        // trên biến động node_modules (install, ghi
                        // store .magicore) mà không ai sửa được bằng cách
                        // sửa file. Reload chỉ hữu ích cho edit con
                        // người; biến động dưới node_modules/.magicore/
                        // dist/build là nhiễu máy móc. Lọc ở tầng
                        // EVENT: mọi path của event phải KHÔNG chứa
                        // component nhiễu.)
                        let signal = event.paths.iter().all(|p| {
                            !p.components().any(|c| {
                                matches!(
                                    c.as_os_str().to_str(),
                                    Some("node_modules" | ".magicore" | "dist" | "build")
                                )
                            })
                        });
                        if signal {
                            let n = version.fetch_add(1, Ordering::SeqCst) + 1;
                            // Observable rebuild (vite parity): log every
                            // applied edit to STDOUT so operators — and
                            // health probes — see the watcher working.
                            // Silent rebuilds look dead from outside.
                            // (Log rebuild ra stdout như vite.)
                            let files = event
                                .paths
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ");
                            println!("[MgDevServer] hmr update {files} (version {n})");
                            let _ = tx.send(HmrEvent::Reload);
                        }
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

    // Subscription-ready greeting (Gate 11, vòng-11 — found by the WS
    // evidence test): a broadcast channel has NO replay — an event fired
    // between the 101 handshake and the server-side `subscribe()` is
    // LOST forever, so a client that edits immediately after connecting
    // can silently miss the reload. Sending `{"type":"connected"}` once
    // the subscription is live gives clients (and tests) a deterministic
    // barrier: edit only after "connected", never miss an event.
    // (Lời chào sẵn-sàng-đăng-ký: broadcast channel KHÔNG replay — event
    // kích giữa handshake 101 và `subscribe()` phía server bị MẤT vĩnh
    // viễn, nên client sửa file ngay sau khi connect có thể âm thầm lỡ
    // reload. Gửi `{"type":"connected"}` khi subscription đã sống cho
    // client (và test) một rào chắn tất định: chỉ sửa sau "connected",
    // không bao giờ lỡ event.)
    {
        let greeting = serde_json::json!({ "type": "connected" }).to_string();
        if socket.send(Message::Text(greeting)).await.is_err() {
            return; // Client disconnected before the barrier.
        }
    }

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
        if (data.type === 'connected') {
            console.log('[MgDevServer] HMR connected.');
            return;
        }
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
