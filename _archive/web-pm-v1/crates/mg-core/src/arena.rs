use bumpalo::Bump;
use std::cell::RefCell;

pub struct ArenaContext {
    arena: RefCell<Bump>,
}

impl ArenaContext {
    pub fn new() -> Self {
        Self {
            arena: RefCell::new(Bump::new()),
        }
    }

    pub fn reset(&self) {
        *self.arena.borrow_mut() = Bump::new();
    }

    pub fn alloc_str(&self, s: &str) -> &str {
        let arena = self.arena.borrow();
        let r = arena.alloc_str(s);
        let ptr = r.as_ptr();
        let len = r.len();
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) }
    }

    pub fn alloc_slice<T: Copy>(&self, slice: &[T]) -> &[T] {
        let arena = self.arena.borrow();
        let r = arena.alloc_slice_copy(slice);
        let ptr = r.as_ptr();
        let len = r.len();
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }

    pub fn bump(&self) -> impl std::ops::Deref<Target = Bump> + '_ {
        self.arena.borrow()
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
    fn test_arena_alloc_str() {
        let ctx = ArenaContext::new();
        let s = ctx.alloc_str("hello");
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_arena_reset() {
        let ctx = ArenaContext::new();
        let s = ctx.alloc_str("hello");
        assert_eq!(s, "hello");
        ctx.reset();
        let s2 = ctx.alloc_str("world");
        assert_eq!(s2, "world");
    }

    #[test]
    fn test_arena_multiple_allocs() {
        let ctx = ArenaContext::new();
        let a = ctx.alloc_str("aaaa");
        let b = ctx.alloc_str("bbbb");
        assert_eq!(a, "aaaa");
        assert_eq!(b, "bbbb");
    }
}
