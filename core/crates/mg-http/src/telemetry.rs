//! Telemetry opt-in (P2 — thiết kế trước, bật sau; 18 §4)
//! (Zero-telemetry mặc định: `MEGAGATE_TELEMETRY=0`/absent → KHÔNG gửi gì,
//!  không hỏi lần đầu. Bật → queue ≤100 events, batch ghi local jsonl;
//!  CHƯA có endpoint gửi ra ngoài — chỉ minh bạch dữ liệu sắp gửi.)

use serde::Serialize;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

const QUEUE_CAP: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct TelemetryEvent {
    pub mg_ver: String,
    pub os: String,
    pub arch: String,
    pub cmd: String,
    pub duration_ms: u64,
    pub exit_code: i32,
}

/// Opt-in tuyệt đối: chỉ bật khi env rõ ràng "1"/"on"/"true"
pub fn enabled() -> bool {
    match std::env::var("MEGAGATE_TELEMETRY") {
        Ok(v) => matches!(v.as_str(), "1" | "on" | "true" | "yes"),
        Err(_) => false,
    }
}

/// Queue thread-safe ≤100 events; không gửi — ghi local jsonl khi flush
pub struct Telemetry {
    queue: Mutex<VecDeque<TelemetryEvent>>,
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn record(&self, ev: TelemetryEvent) {
        if !enabled() {
            return;
        }
        let mut q = self.queue.lock().unwrap();
        if q.len() >= QUEUE_CAP {
            q.pop_front(); // ponytail: queue bounded, drop cũ nhất
        }
        q.push_back(ev);
    }

    /// Ghi batch xuống file local (minh bạch — user xem trước khi gửi)
    pub fn flush(&self) -> std::io::Result<PathBuf> {
        let dir = default_log_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("events.jsonl");
        let q = self.queue.lock().unwrap();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        for ev in q.iter() {
            let line = serde_json::to_string(ev)?;
            use std::io::Write;
            writeln!(file, "{line}")?;
        }
        Ok(path)
    }
}

fn default_log_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("megagate/telemetry");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".megagate/telemetry")
}