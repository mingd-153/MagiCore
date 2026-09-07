//! `mgc dev` — router: core detect → file con (v5: LỆNH = folder, CORE = file).
//! Port cố định per core từ bảng tập trung `dev_port.rs` (RULE §13 — hoán vị 4·3·1·5).

#[cfg(feature = "ai")]
pub mod ai;
#[cfg(feature = "ai")]
pub mod ai_docker;
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "cicd")]
pub mod cicd;
#[cfg(feature = "clo")]
pub mod clo;
#[cfg(feature = "iot")]
pub mod iot;
pub mod web;

// Router `run`/`run_with_port`/`run_multi` đã bị loại: dispatch live
// (dispatch/common.rs → commands::dev::run) gọi trực tiếp từng dev::<core>;
// bảng port trung tâm giờ được tiêu thụ tại commands::dev::run + web::dev_targets.
// Removed legacy routers: live dispatch calls each dev subcommand directly.
