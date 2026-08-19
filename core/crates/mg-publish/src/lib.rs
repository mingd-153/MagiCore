//! mg-publish — publish client (MegaGate)
//! Publishes tarballs to the private registry with auth + retry.
//! (Client publish: tarball + auth + retry — sys-mg/01)
//!
//! Modules: auth (resolution), registry (select), publish (client).

pub mod auth;
