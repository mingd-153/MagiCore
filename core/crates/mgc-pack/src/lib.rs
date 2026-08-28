//! mgc-pack — pack tarball stream P1 (MagiCore)
//! Streams packed tarball artifacts with content hashing.
//! (Đóng gói tarball dạng stream — P1, sys-mgc/02 §4, sys-mgc/01 §4.5-4.6)
//!
//! Modules: ignore (file selection), manifest (sanitize), tarball (builder + hashes).

pub mod ignore;
pub mod manifest;
pub mod tarball;
