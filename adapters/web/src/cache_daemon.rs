//! In-Memory Caching Daemon for sub-millisecond warm installs
use mg_types::PackageId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub struct MemoryCacheDaemon {
    entries: Mutex<HashMap<String, Arc<[u8]>>>,
}

impl MemoryCacheDaemon {
    pub fn global() -> &'static Self {
        static INSTANCE: OnceLock<MemoryCacheDaemon> = OnceLock::new();
        INSTANCE.get_or_init(|| MemoryCacheDaemon {
            entries: Mutex::new(HashMap::new()),
        })
    }

    pub fn get(&self, id: &PackageId) -> Option<Arc<[u8]>> {
        let key = format!("{}@{}", id.name_str(), id.version());
        let guard = self.entries.lock().ok()?;
        guard.get(&key).cloned()
    }

    pub fn insert(&self, id: &PackageId, data: &[u8]) {
        let key = format!("{}@{}", id.name_str(), id.version());
        if let Ok(mut guard) = self.entries.lock() {
            if guard.len() > 10000 {
                guard.clear();
            }
            guard.insert(key, Arc::from(data));
        }
    }
}
