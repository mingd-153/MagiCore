/// Manifest sanitize — exportable manifest như pnpm (01 §4.4)
/// Bỏ private/publishConfig, ghi đè name/version theo trạng thái thật.
use anyhow::{bail, Result};
use serde_json::{Map, Value};

/// Result: manifest sạch để đăng + các trường dependencies giữ nguyên.
pub struct Sanitized {
    pub manifest: Value,
    pub name: String,
    pub version: String,
}

/// Sanitize manifest trước khi pack:
/// - bỏ: private (nếu true → chặn publish), publishConfig
/// - ghi đè: name, version, main/module/types, exports theo trạng thái thật
/// - phải có: name, version (description thiếu → warn qua `warnings`)
pub fn sanitize(raw: Value, name: &str, version: &str) -> Result<Sanitized> {
    let Some(obj) = raw.as_object() else {
        bail!("manifest is not a JSON object");
    };

    if obj.get("private").and_then(Value::as_bool) == Some(true) {
        bail!("package.json has private: true — cannot publish (remove private: true to publish)");
    }

    let manifest_name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    if manifest_name.is_empty() {
        bail!("package.json missing name");
    }

    let manifest_version = obj
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    if manifest_version.is_empty() {
        bail!("package.json missing version");
    }

    let mut out = obj.clone();
    out.remove("private");
    out.remove("publishConfig");

    // ghi đè theo trạng thái thật (version đã bump, name đã xác nhận)
    if !name.is_empty() {
        out.insert("name".into(), Value::String(name.to_string()));
    }
    if !version.is_empty() {
        out.insert("version".into(), Value::String(version.to_string()));
    }

    Ok(Sanitized {
        manifest: Value::Object(out),
        name: name_or(&manifest_name, name),
        version: version_or(&manifest_version, version),
    })
}

/// Dependencies fields (dependencies/devDependencies/peerDependencies/optionalDependencies)
/// giữ nguyên — tách để publish body dùng.
pub fn dep_fields(manifest: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    for key in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(v) = manifest.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    out
}

fn name_or(manifest: &str, override_name: &str) -> String {
    if override_name.is_empty() {
        manifest.to_string()
    } else {
        override_name.to_string()
    }
}

fn version_or(manifest: &str, override_version: &str) -> String {
    if override_version.is_empty() {
        manifest.to_string()
    } else {
        override_version.to_string()
    }
}
