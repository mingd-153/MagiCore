//! MGPM integration test suite
//!
//! Tests the full package management pipeline end-to-end using
//! mock registries, temporary filesystems, and in-process fixtures.

mod importer_test;
mod installer_test;
mod linker_test;
mod lockfile_test;
mod plugin_test;
mod profiler_test;
mod registry_auth_test;
mod resolver_test;
mod store_test;
pub mod test_utils;
mod workspace_test;
