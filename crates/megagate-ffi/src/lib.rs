use megagate_core::MegagateCore;
use megagate_types::config::MegagateConfig;
use megagate_types::error::{MegagateError, Result};
use napi_derive::napi;

uniffi::setup_scaffolding!("megagate");

#[uniffi::export]
pub fn megagate_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[uniffi::export]
pub async fn megagate_install(
    project_dir: String,
    config_json: Option<String>,
) -> Result<String> {
    let config = if let Some(json) = config_json {
        serde_json::from_str(&json).map_err(|e| MegagateError::ConfigError(e.to_string()))?
    } else {
        MegagateConfig::default()
    };

    let core = MegagateCore::new(config).await?;
    let result = core.install(&project_dir).await?;
    Ok(serde_json::to_string(&result).unwrap())
}

#[uniffi::export]
pub async fn megagate_add(
    project_dir: String,
    package_spec: String,
    dev: bool,
) -> Result<String> {
    let config = MegagateConfig::default();
    let core = MegagateCore::new(config).await?;
    let result = core.add(&project_dir, &package_spec, dev).await?;
    Ok(serde_json::to_string(&result).unwrap())
}

#[uniffi::export]
pub async fn megagate_update(
    project_dir: String,
    package_name: Option<String>,
) -> Result<String> {
    let config = MegagateConfig::default();
    let core = MegagateCore::new(config).await?;
    let result = core.update(&project_dir, package_name.as_deref()).await?;
    Ok(serde_json::to_string(&result).unwrap())
}

#[uniffi::export]
pub async fn megagate_remove(
    project_dir: String,
    package_name: String,
) -> Result<String> {
    let config = MegagateConfig::default();
    let core = MegagateCore::new(config).await?;
    let result = core.remove(&project_dir, &package_name).await?;
    Ok(serde_json::to_string(&result).unwrap())
}

#[uniffi::export]
pub async fn megagate_list(
    project_dir: String,
    depth: u32,
) -> Result<String> {
    let config = MegagateConfig::default();
    let core = MegagateCore::new(config).await?;
    let result = core.list(&project_dir, depth).await?;
    Ok(serde_json::to_string(&result).unwrap())
}

#[uniffi::export]
pub async fn megagate_audit(
    project_dir: String,
) -> Result<String> {
    let config = MegagateConfig::default();
    let core = MegagateCore::new(config).await?;
    let result = core.audit(&project_dir).await?;
    Ok(serde_json::to_string(&result).unwrap())
}

#[uniffi::export]
pub async fn megagate_lock_verify(
    project_dir: String,
) -> Result<String> {
    let config = MegagateConfig::default();
    let core = MegagateCore::new(config).await?;
    let result = core.verify_lockfile(&project_dir).await?;
    Ok(serde_json::to_string(&result).unwrap())
}

#[napi]
pub async fn napi_megagate_install(
    project_dir: String,
    config_json: Option<String>,
) -> napi::Result<String> {
    let result = megagate_install(project_dir, config_json).await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result)
}

#[napi]
pub async fn napi_megagate_add(
    project_dir: String,
    package_spec: String,
    dev: bool,
) -> napi::Result<String> {
    let result = megagate_add(project_dir, package_spec, dev).await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result)
}

#[napi]
pub async fn napi_megagate_update(
    project_dir: String,
    package_name: Option<String>,
) -> napi::Result<String> {
    let result = megagate_update(project_dir, package_name).await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result)
}

#[napi]
pub async fn napi_megagate_remove(
    project_dir: String,
    package_name: String,
) -> napi::Result<String> {
    let result = megagate_remove(project_dir, package_name).await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result)
}

#[napi]
pub async fn napi_megagate_list(
    project_dir: String,
    depth: u32,
) -> napi::Result<String> {
    let result = megagate_list(project_dir, depth).await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result)
}

#[napi]
pub async fn napi_megagate_lock_verify(
    project_dir: String,
    config_json: Option<String>,
) -> napi::Result<String> {
    let result = megagate_lock_verify(project_dir).await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(result)
}

// wasm-bindgen functions (TODO: fix when needed)
// #[wasm_bindgen::prelude::wasm_bindgen]
// pub async fn wasm_megagate_install(
//     project_dir: String,
//     config_json: Option<String>,
// ) -> Result<String, JsValue> {
//     let result = megagate_install(project_dir, config_json).await
//         .map_err(|e| JsValue::from_str(&e.to_string()))?;
//     Ok(result)
// }
//
// #[wasm_bindgen::prelude::wasm_bindgen]
// pub async fn wasm_megagate_add(
//     project_dir: String,
//     package_spec: String,
//     dev: bool,
// ) -> Result<String, JsValue> {
//     let result = megagate_add(project_dir, package_spec, dev).await
//         .map_err(|e| JsValue::from_str(&e.to_string()))?;
//     Ok(result)
// }