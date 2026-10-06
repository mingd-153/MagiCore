#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-publish — publish client (MagiCore)
//! Publishes tarballs to the private registry with auth + retry.
//! Publish client for tarballs, authentication, and retries — client publish tarball, xác thực và retry.
//!
//! Modules: auth (resolution), registry (select), publish (client).

pub mod auth;
