//! Global allocator configuration

#[cfg(any(target_family = "unix", target_family = "windows"))]
#[cfg(not(target_env = "musl"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
