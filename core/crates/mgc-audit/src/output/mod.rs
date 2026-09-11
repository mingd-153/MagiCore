//! `output` — Machine-readable audit report formats with a stable,
//! versioned schema so CI and GitHub Security ingest can rely on the
//! shape (Tech Lead 2026-09-09: JSON/SARIF/CycloneDX evidence).
//! Định dạng report audit cho máy — schema ổn định, có version để CI và
//! GitHub Security ingest dựa vào được.

pub mod cyclonedx;
pub mod json;
pub mod sarif;
