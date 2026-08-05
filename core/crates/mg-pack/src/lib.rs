//! mg-pack — pack tarball stream P1 (MegaGate)
//! Streams packed tarball artifacts with content hashing.
//! (Đóng gói tarball dạng stream — P1, sys-mg/02 §4, sys-mg/01 §4.5-4.6)
//!
//! Modules: ignore (file selection), manifest (sanitize), tarball (builder + hashes).

pub mod ignore;
pub mod manifest;
pub mod tarball;
