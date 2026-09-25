//! Workspace detection, graph, filter, topo (T4).

pub mod catalog;
pub mod computation_cache;
mod discover;
mod filter;
mod graph;
mod topo;

pub use catalog::{WorkspaceCatalogs, load_workspace_catalogs, resolve_catalog_specifier};
pub use computation_cache::{
    PackageBuildCache, check_package_build_freshness, compute_composite_hash,
    compute_package_source_hash, load_package_build_cache, save_package_build_cache,
};
pub use discover::{DiscoverOptions, build_workspace_graph, discover_workspace_targets};
pub use filter::filter_matches;
pub use graph::{
    WorkspaceEdge, WorkspaceGraph, WorkspaceNode, WorkspacePackageManifest, read_package_manifest,
};
pub use topo::{TopoError, topo_levels};
