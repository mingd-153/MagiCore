//! dispatch — run() entry + phân tuyến: engine → common / per_core (v5).

mod bare;
mod common;
mod engine;
mod per_core;
mod types;

mod core;

pub use engine::run;
