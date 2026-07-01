use std::sync::Mutex;
use bumpalo::Bump;

pub struct ArenaContext {
    bump: Mutex<Bump>,
}

impl ArenaContext {
    pub fn new() -> Self {
        Self { bump: Mutex::new(Bump::new()) }
    }

    pub fn reset(&self) {
        *self.bump.lock().unwrap() = Bump::new();
    }

    pub fn alloc_str(&self, s: &str) -> String {
        let bump = self.bump.lock().unwrap();
        bump.alloc_str(s).to_string()
    }

    pub fn inner(&self) -> std::sync::MutexGuard<'_, Bump> {
        self.bump.lock().unwrap()
    }
}

impl Default for ArenaContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_alloc_str_basic() {
        let arena = ArenaContext::new();
        let s = arena.alloc_str("hello");
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_arena_alloc_str_multiple() {
        let arena = ArenaContext::new();
        let a = arena.alloc_str("foo");
        let b = arena.alloc_str("bar");
        assert_eq!(a, "foo");
        assert_eq!(b, "bar");
        // Different strings, same arena
        assert_ne!(a, b);
    }

    #[test]
    fn test_arena_alloc_str_empty() {
        let arena = ArenaContext::new();
        let s = arena.alloc_str("");
        assert_eq!(s, "");
    }

    #[test]
    fn test_arena_alloc_str_unicode() {
        let arena = ArenaContext::new();
        let s = arena.alloc_str("Tiếng Việt \u{1F600}");
        assert_eq!(s, "Tiếng Việt \u{1F600}");
    }

    #[test]
    fn test_arena_reset() {
        let arena = ArenaContext::new();
        let s1 = arena.alloc_str("before");
        assert_eq!(s1, "before");

        arena.reset();

        let s2 = arena.alloc_str("after");
        assert_eq!(s2, "after");
    }

    #[test]
    fn test_arena_reset_reuses_memory() {
        let arena = ArenaContext::new();
        // Allocate a bunch of strings
        for i in 0..100 {
            let s = arena.alloc_str(&format!("item-{}", i));
            assert_eq!(s, format!("item-{}", i));
        }

        arena.reset();

        // Should work fine after reset
        let s = arena.alloc_str("fresh");
        assert_eq!(s, "fresh");
    }

    #[test]
    fn test_arena_default() {
        let arena: ArenaContext = Default::default();
        let s = arena.alloc_str("default");
        assert_eq!(s, "default");
    }

    #[test]
    fn test_arena_large_string() {
        let arena = ArenaContext::new();
        let large = "a".repeat(10_000);
        let s = arena.alloc_str(&large);
        assert_eq!(s.len(), 10_000);
        assert_eq!(s, large);
    }

    #[test]
    fn test_arena_concurrent_allocs() {
        let arena = ArenaContext::new();
        let arena_ref = &arena;

        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for i in 0..10 {
                handles.push(scope.spawn(move || {
                    for j in 0..100 {
                        let s = arena_ref.alloc_str(&format!("thread-{}-{}", i, j));
                        assert_eq!(s, format!("thread-{}-{}", i, j));
                    }
                }));
            }
        });
    }

    #[test]
    fn test_arena_concurrent_reset() {
        let arena = ArenaContext::new();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..10 {
                    arena.alloc_str("worker");
                }
            });
            scope.spawn(|| {
                arena.reset();
            });
        });

        // Still usable after concurrent reset
        let s = arena.alloc_str("post-reset");
        assert_eq!(s, "post-reset");
    }

    #[test]
    fn test_arena_multiple_resets() {
        let arena = ArenaContext::new();
        for cycle in 0..5 {
            for i in 0..50 {
                let s = arena.alloc_str(&format!("cycle-{}-{}", cycle, i));
                assert_eq!(s, format!("cycle-{}-{}", cycle, i));
            }
            arena.reset();
        }
        let s = arena.alloc_str("final");
        assert_eq!(s, "final");
    }
}
