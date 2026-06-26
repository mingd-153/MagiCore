//! MegaGate Core NAPI bindings
//! Re-exports NAPI functions from megagate-ffi

use megagate_ffi::*;
use napi_derive::napi;

// Wrapper functions to re-export NAPI functions
#[napi]
pub async fn napi_megagate_install(
    project_dir: String,
    config_json: Option<String>,
) -> napi::Result<String> {
    megagate_ffi::napi_megagate_install(project_dir, config_json).await
}

#[napi]
pub async fn napi_megagate_add(
    project_dir: String,
    package_spec: String,
    dev: bool,
) -> napi::Result<String> {
    megagate_ffi::napi_megagate_add(project_dir, package_spec, dev).await
}

#[napi]
pub async fn napi_megagate_update(
    project_dir: String,
    package_name: Option<String>,
) -> napi::Result<String> {
    megagate_ffi::napi_megagate_update(project_dir, package_name).await
}

#[napi]
pub async fn napi_megagate_remove(
    project_dir: String,
    package_name: String,
) -> napi::Result<String> {
    megagate_ffi::napi_megagate_remove(project_dir, package_name).await
}

#[napi]
pub async fn napi_megagate_list(
    project_dir: String,
    depth: u32,
) -> napi::Result<String> {
    megagate_ffi::napi_megagate_list(project_dir, depth).await
}

#[napi]
pub async fn napi_megagate_lock_verify(
    project_dir: String,
    config_json: Option<String>,
) -> napi::Result<String> {
    megagate_ffi::napi_megagate_lock_verify(project_dir, config_json).await
}
