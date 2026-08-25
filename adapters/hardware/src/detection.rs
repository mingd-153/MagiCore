//! Hardware adapter detection helpers.
//! Hardware là add-on cross-core nên detect qua mgc.toml của project MagiCore.

use std::path::Path;

pub(crate) fn manifest_is_any_mg(root: &Path) -> bool {
    root.join("mgc.toml").is_file()
}
