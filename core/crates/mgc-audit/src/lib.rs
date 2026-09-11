//! `mgc-audit` — Shared security-audit engine for all MagiCore cores.
//! Adapters detect manifests → build an AuditPlan → this engine executes
//! the scanners and aggregates one report under one policy contract.
//! Engine audit chia sẻ cho mọi core: adapter nhận diện manifest, dựng
//! AuditPlan; engine chạy scanner và tổng hợp một report theo một hợp
//! đồng policy duy nhất.

pub mod aggregate;
pub mod contract;
pub mod engine;
pub mod output;
pub mod polyglot;
pub mod scanners;

pub use aggregate::aggregate_reports;
pub use contract::ScanStep;
pub use engine::AuditPlan;
pub use polyglot::{audit_polyglot, plan_for_shared_manifests};
