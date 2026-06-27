//! Global allocator configuration

#[cfg(all(target_family = "unix", not(target_env = "musl"), not(target_os = "macos")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(target_family = "windows"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_os = "macos")]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;
