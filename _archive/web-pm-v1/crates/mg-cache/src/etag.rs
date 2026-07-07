use std::collections::HashMap;

use crate::CacheError;

pub struct ETagStore {
    etags: HashMap<String, String>,
    bodies: HashMap<String, Vec<u8>>,
}

impl ETagStore {
    pub fn new() -> Self {
        Self {
            etags: HashMap::new(),
            bodies: HashMap::new(),
        }
    }

    pub fn get_etag(&self, url: &str) -> Option<&str> {
        self.etags.get(url).map(|s| s.as_str())
    }

    pub fn store(&mut self, url: &str, etag: &str, body: &[u8]) -> Result<(), CacheError> {
        self.etags.insert(url.to_string(), etag.to_string());
        self.bodies.insert(url.to_string(), body.to_vec());
        Ok(())
    }

    pub fn get_body(&self, url: &str) -> Option<&[u8]> {
        self.bodies.get(url).map(|v| v.as_slice())
    }

    pub fn is_fresh(&self, url: &str) -> bool {
        self.etags.contains_key(url)
    }

    pub fn clear(&mut self) {
        self.etags.clear();
        self.bodies.clear();
    }
}

impl Default for ETagStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etag_store_roundtrip() {
        let mut store = ETagStore::new();
        store
            .store(
                "https://registry.npmjs.org/lodash",
                "\"abc123\"",
                b"{\"name\":\"lodash\"}",
            )
            .unwrap();

        assert_eq!(
            store.get_etag("https://registry.npmjs.org/lodash"),
            Some("\"abc123\"")
        );
        assert_eq!(
            store.get_body("https://registry.npmjs.org/lodash"),
            Some(b"{\"name\":\"lodash\"}" as &[u8])
        );
    }

    #[test]
    fn test_etag_store_miss() {
        let store = ETagStore::new();
        assert!(store.get_etag("https://example.com/nonexistent").is_none());
        assert!(store.get_body("https://example.com/nonexistent").is_none());
    }

    #[test]
    fn test_etag_clear() {
        let mut store = ETagStore::new();
        store.store("url1", "\"e1\"", b"body1").unwrap();
        store.store("url2", "\"e2\"", b"body2").unwrap();
        assert!(store.is_fresh("url1"));
        store.clear();
        assert!(!store.is_fresh("url1"));
    }
}
