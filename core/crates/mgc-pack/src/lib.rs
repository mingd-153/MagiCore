//! mgc-pack — pack tarball stream P1 (MagiCore)
//! Streams packed tarball artifacts with content hashing.
//! Stream package tarballs — đóng gói tarball theo luồng để tránh giữ toàn bộ payload trong bộ nhớ.
//!
//! Modules: ignore (file selection), manifest (sanitize), tarball (builder + hashes).

pub mod ignore;
pub mod manifest;
pub mod tarball;
