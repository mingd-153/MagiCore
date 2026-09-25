//! Native-only boundary retained for downstream API compatibility.
//! Biên native-only giữ lại để tương thích API downstream.

use mgc_types::MgResult;
use std::path::Path;

pub fn check_pip_allowed(_root: &Path, name: &str) -> MgResult<()> {
    Err(mgc_types::MgError::Unsupported {
        core: "lib",
        capability: "external pip compatibility",
        guidance: format!(
            "MagiCore never invokes pip for '{name}'; use a native lane with resolver, lock, verified store, and materializer support"
        ),
    })
}
