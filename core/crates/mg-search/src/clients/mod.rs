//! Search clients for all registries - shared across ALL cores
//! Clients tìm kiếm cho tất cả registries - dùng chung CHO MỌI core

mod npm_client;
mod crates_client;
mod go_client;
mod pypi_client;

pub use npm_client::NpmSearchClient;
pub use crates_client::CratesSearchClient;
pub use go_client::GoSearchClient;
pub use pypi_client::PyPISearchClient;
