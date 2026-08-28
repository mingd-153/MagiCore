//! Workspace detection, graph, filter, topo (T4).

pub mod catalog;
pub mod computation_cache;
mod discover;
mod filter;
mod graph;
mod topo;

pub use catalog::{load_workspace_catalogs, resolve_catalog_specifier, WorkspaceCatalogs};
pub use computation_cache::{
    check_package_build_freshness, compute_composite_hash, compute_package_source_hash,
    load_package_build_cache, save_package_build_cache, PackageBuildCache,
};
pub use discover::{build_workspace_graph, discover_workspace_targets, DiscoverOptions};
pub use filter::filter_matches;
pub use graph::{
    read_package_manifest, WorkspaceEdge, WorkspaceGraph, WorkspaceNode, WorkspacePackageManifest,
};
pub use topo::{topo_levels, TopoError};
