/// Entry point for the webapp core module.
///
/// Re‑exports the main sub‑modules so they can be imported as
/// `use crate::webapp_core::*;`.

pub mod api;
pub mod service;
pub mod domain;
pub mod repository;
pub mod config;
pub mod util;
pub mod app;
pub mod web;
pub mod shared;
pub mod tests;
