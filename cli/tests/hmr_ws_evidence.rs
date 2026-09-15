//! WebSocket HMR evidence (Gate 11, vòng-11 verdict): the audit rejected
//! HTTP-log-only "HMR tests" — a real HMR proof needs a WebSocket 101
//! handshake, an update event AFTER a source edit, a re-fetch of the
//! transpiled module showing the NEW source marker, and a clean teardown
//! (no listening port left). This test drives the REAL MgDevServer over
//! the REAL network stack.
//!
//! The port is DYNAMIC (bind :0) — RULE §13's fixed table is for the
//! `mgc dev` UX default; tests must never fight over 4315.
//!
//! (Bằng chứng WS HMR (phán quyết vòng-11): audit từ chối test HMR chỉ
//! HTTP/log — chứng minh HMR thật cần handshake WebSocket 101, nhận
//! update event SAU KHI sửa source, fetch lại module transpile thấy
//! marker source MỚI, và teardown sạch (không còn port lắng nghe).
//! Test này chạy trên MgDevServer THẬT qua network stack THẬT.
//!
//! Port là ĐỘNG (bind :0) — bảng cố định RULE §13 là default UX của
//! `mgc dev`; test không được giành port 4315.)

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use futures_util::StreamExt;
use mgc::bundler::dev_server::{DevServerConfig, MgDevServer};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Pick a free port by binding :0 — the OS-allocated dynamic port, never
/// the RULE §13 fixed defaults (a test holding 4315 would break a
/// concurrently-running `mgc dev`).
/// (Chọn port tự do bằng bind :0 — port động OS cấp, không bao giờ port
/// cố định RULE §13 (test giữ 4315 sẽ làm hỏng `mgc dev` chạy song song).)
fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn ws_connect(port: u16) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let (stream, resp) = tokio_tungstenite::connect_async(
        format!("ws://127.0.0.1:{port}/@magicore/hmr")
            .into_client_request()
            .unwrap(),
    )
    .await
    .expect("WebSocket handshake must succeed");
    // THE 101 proof: the HTTP upgrade response status is
    // SWITCHING_PROTOCOLS — anything else (4xx page, redirect) is not a
    // WebSocket.
    // (Bằng chứng 101: status của response upgrade là SWITCHING_PROTOCOLS
    // — khác (trang 4xx, redirect) thì không phải WebSocket.)
    assert_eq!(
        resp.status(),
        StatusCode::SWITCHING_PROTOCOLS,
        "HMR handshake must be a real 101 Switching Protocols upgrade"
    );
    stream
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hmr_websocket_101_update_event_and_recompiled_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("site");
    std::fs::create_dir_all(root.join("src")).unwrap();
    // index.html loads the entry module.
    // (index.html nạp module entry.)
    std::fs::write(
        root.join("index.html"),
        r#"<html><head><title>hmr</title></head><body><script type="module" src="/src/main.ts"></script></body></html>"#,
    )
    .unwrap();
    // Source marker v1 — the compiled output must carry it verbatim.
    // (Marker source v1 — output biên dịch phải chứa nguyên văn.)
    std::fs::write(
        root.join("src").join("main.ts"),
        "export const MARKER = 'hmr-marker-v1';\nconsole.log(MARKER);\n",
    )
    .unwrap();

    let port = free_port();
    let server_root = root.clone();
    let server = tokio::spawn(async move {
        let config = DevServerConfig {
            root: server_root,
            entry: "src/main.ts".into(),
            host: "localhost".to_string(),
            port,
        };
        MgDevServer::new(config).serve().await
    });
    // Readiness probe: poll the HTTP endpoint until the server answers.
    // (Đo sẵn sàng: poll endpoint HTTP tới khi server trả lời.)
    let mut ready = false;
    for _ in 0..100 {
        if reqwest::get(format!("http://127.0.0.1:{port}/"))
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready, "dev server must become ready on the dynamic port");

    // (1) Baseline: the transpiled module serves marker v1.
    // ((1) Baseline: module transpile phục vụ marker v1.)
    let v1 = reqwest::get(format!("http://127.0.0.1:{port}/src/main.ts"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        v1.contains("hmr-marker-v1"),
        "baseline module serves v1 marker, body: {v1:?}"
    );

    // (2) Real WebSocket client: 101 handshake.
    // ((2) Client WebSocket thật: handshake 101.)
    let mut ws = ws_connect(port).await;

    // (2b) Wait for the server's "connected" greeting — the deterministic
    // subscribe-barrier (Gate 11): editing before the server-side
    // subscription is live can MISS the one-shot broadcast event.
    // ((2b) Chờ lời chào "connected" của server — rào chắn subscribe tất
    // định: sửa file trước khi subscription phía server sống có thể LỠ
    // broadcast event one-shot.)
    let connected = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(txt))) => {
                    let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
                    if v["type"] == "connected" {
                        return true;
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(connected, "HMR socket must receive the connected greeting");

    // (3) Edit the source → watcher fires → server pushes a reload event.
    // ((3) Sửa source → watcher kích → server đẩy event reload.)
    std::fs::write(
        root.join("src").join("main.ts"),
        "export const MARKER = 'hmr-marker-v2';\nconsole.log(MARKER);\n",
    )
    .unwrap();

    // The notify watcher is async; the event must arrive within a bounded
    // window (CI-safe). timeout() — not sleep() — keeps the test fast on
    // fast machines and bounded on slow ones.
    // (Watcher notify là async; event phải đến trong cửa sổ giới hạn (an
    // toàn CI). timeout() — không sleep() — test nhanh trên máy nhanh và
    // vẫn có chặn trên máy chậm.)
    let reload_seen = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(txt))) => {
                    let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
                    if v["type"] == "reload" {
                        return true;
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(
        reload_seen,
        "HMR socket must receive a reload event after a source edit (Gate 11 runtime evidence)"
    );

    // (4) The RE-FETCHED module carries the NEW marker: the server now
    // transpiles (or serves from the re-validated cache) the v2 source.
    // ((4) Module FETCH LẠI mang marker MỚI: server giờ transpile (hoặc
    // phục vụ từ cache đã re-validate) source v2.)
    let mut v2 = String::new();
    for _ in 0..100 {
        let body = reqwest::get(format!("http://127.0.0.1:{port}/src/main.ts"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        if body.contains("hmr-marker-v2") {
            v2 = body;
            break;
        }
        // Stale for now — fs event propagation may lag one poll.
        // (Chưa mới — event fs có thể trễ một poll.)
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        v2.contains("hmr-marker-v2"),
        "re-fetched module must carry the NEW source marker (marker-based rebuild proof), body: {v2}"
    );

    // (5) Teardown: stop the server, then the port must be REUSABLE (no
    // lingering listener) — the audit's teardown requirement.
    // ((5) Teardown: dừng server, port phải DÙNG LẠI ĐƯỢC (không còn
    // listener) — yêu cầu teardown của audit.)
    server.abort();
    server.await.ok();
    // Give the OS a bounded moment to release the socket, then PROVE it:
    // a fresh bind on the same port must succeed.
    // (Cho OS khoảnh khắc giới hạn để nhả socket, rồi CHỨNG MINH: bind
    // mới trên cùng port phải thành công.)
    let released = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or(false);
    assert!(released, "port {port} must be free after server teardown");
}
