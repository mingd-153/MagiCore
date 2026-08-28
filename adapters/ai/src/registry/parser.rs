//! Registry URL parser cho model IDs.

use mgc_types::{MgError, MgResult};

/// Parse HuggingFace model URL/ID
/// Format: "org/model" hoặc "https://huggingface.co/org/model"
pub fn parse_hf_id(input: &str) -> MgResult<(String, Option<String>)> {
    let clean = input
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("huggingface.co/");

    let parts: Vec<&str> = clean.split('/').collect();

    if parts.len() >= 2 {
        let repo_id = format!("{}/{}", parts[0], parts[1]);
        let revision = parts.get(3).map(|s| s.to_string()); // /resolve/{rev}/
        Ok((repo_id, revision))
    } else {
        Err(MgError::Other(format!("Invalid HuggingFace ID: {}", input)))
    }
}

/// Parse TensorFlow Hub model URL
/// Format: "https://tfhub.dev/google/model/version"
pub fn parse_tfhub_url(url: &str) -> MgResult<(String, String)> {
    let clean = url
        .trim_start_matches("https://")
        .trim_start_matches("tfhub.dev/");

    let parts: Vec<&str> = clean.split('/').collect();

    if parts.len() >= 3 {
        let model_id = parts[..parts.len() - 1].join("/");
        let version = parts[parts.len() - 1].to_string();
        Ok((model_id, version))
    } else {
        Err(MgError::Other(format!("Invalid TFHub URL: {}", url)))
    }
}

/// Parse ONNX model path
/// Format: "vision/classification/resnet/model.onnx"
pub fn parse_onnx_path(path: &str) -> MgResult<String> {
    if path.contains(".onnx") {
        Ok(path.to_string())
    } else {
        Err(MgError::Other(format!("Invalid ONNX path: {}", path)))
    }
}

#[cfg(test)]
#[path = "test/parser_tests.rs"]
mod tests;
