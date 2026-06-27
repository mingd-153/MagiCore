//! MGPM integration test suite
//!
//! Tests the full package management pipeline end-to-end using
//! mock registries, temporary filesystems, and in-process fixtures.

pub mod test_utils;
mod resolver_test;
mod installer_test;
mod store_test;
mod lockfile_test;
mod registry_auth_test;
mod workspace_test;
mod plugin_test;
mod importer_test;
mod profiler_test;
