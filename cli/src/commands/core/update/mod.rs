//! `mgc update` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

#[cfg(feature = "ai")]
pub mod ai;
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "cicd")]
pub mod cicd;
#[cfg(feature = "clo")]
pub mod clo;
#[cfg(feature = "game")]
pub mod game;
#[cfg(feature = "iot")]
pub mod iot;
#[cfg(feature = "lib")]
pub mod library;
pub mod web;

// Router `run(core, ...)` đã bị loại: dispatch live (dispatch/core/package_ops.rs)
// gọi trực tiếp update::<core> — router này trùng lắp, không còn caller.
// Removed legacy router: live dispatch calls each update subcommand directly.
