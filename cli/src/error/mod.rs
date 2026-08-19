//! Centralized error handling for the CLI (RULE §5 / user 2026-08-19).
//! Mọi error message của CLI định nghĩa tập trung tại đây — không rải rác
//! bail!/anyhow! inline trong các command files.

pub mod messages;

pub use messages::*;
