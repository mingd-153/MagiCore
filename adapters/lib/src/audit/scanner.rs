//! `audit/scanner.rs` — Scanner facade for the lib adapter.
//! The implementations live in the shared engine (`mgc_audit::scanners`)
//! so every adapter runs identical cargo-audit/pip-audit logic; this
//! module only re-exports for backwards compatibility with internal
//! callers and keeps the parser regression tests in place.
//! Facade scanner cho lib adapter — bản hiện thực nằm ở engine chung
//! (mgc_audit::scanners) để mọi adapter chạy cùng logic; module này chỉ
//! re-export và giữ chỗ cho test regression parser.

pub use mgc_audit::scanners::{audit_python, audit_rust};

#[cfg(test)]
#[path = "test/scanner_test.rs"]
mod tests;
