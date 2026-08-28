#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mgc-publish — publish client (MagiCore)
//! Publishes tarballs to the private registry with auth + retry.
//! (Client publish: tarball + auth + retry — sys-mgc/01)
//!
//! Modules: auth (resolution), registry (select), publish (client).

pub mod auth;
