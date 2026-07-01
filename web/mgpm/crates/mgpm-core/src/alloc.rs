//! Allocator configuration
//!
//! Binary crates should set the global allocator using:
//! ```ignore
//! #[global_allocator]
//! static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
//! ```
//!
//! Platform guards required:
//! - Exclude musl (Alpine Linux uses its own allocator)
//! - Exclude miri (C code incompatible)
//! - Exclude WASM (target_family = "wasm")
