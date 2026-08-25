//! Memory optimization utilities for cache operations
//! Tiện ích tối ưu bộ nhớ cho cache

use std::sync::Arc;

/// Shared string pool to reduce allocations — Pool chuỗi shared giảm allocation
pub struct StringPool {
    strings: dashmap::DashMap<Arc<str>, ()>,
}

impl StringPool {
    pub fn new() -> Self {
        Self {
            strings: dashmap::DashMap::new(),
        }
    }

    /// Intern a string (reuse existing if present) — Intern chuỗi (tái sử dụng nếu có)
    pub fn intern(&self, s: &str) -> Arc<str> {
        if let Some(entry) = self.strings.get(s) {
            return entry.key().clone();
        }

        let arc: Arc<str> = Arc::from(s);
        self.strings.insert(arc.clone(), ());
        arc
    }

    /// Clear pool — Xóa pool
    pub fn clear(&self) {
        self.strings.clear();
    }

    /// Get pool size — Lấy kích thước pool
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if empty — Kiểm tra rỗng
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_pool() {
        let pool = StringPool::new();

        let s1 = pool.intern("hello");
        let s2 = pool.intern("hello");
        let s3 = pool.intern("world");

        // Same string returns same Arc
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(!Arc::ptr_eq(&s1, &s3));

        assert_eq!(pool.len(), 2); // "hello" and "world"
    }
}
