//! `mgc install` — router: core detect → file con (v5: LỆNH = folder, CORE = file).

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
#[cfg(feature = "hardware")]
pub mod hardware;
#[cfg(feature = "iot")]
pub mod iot;
#[cfg(feature = "lib")]
pub mod library;
pub mod web;

// Router `run(core, packages)` đã bị loại: dispatch live (dispatch/core/install.rs)
// gọi trực tiếp install::<core> và tự chạy optimizer — router này trùng lắp, không caller.
// Removed legacy router: live dispatch calls each install subcommand directly.
