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
mod tests {
    use super::*;

    #[test]
    fn test_parse_hf_id_simple() {
        let (repo, rev) = parse_hf_id("openai/gpt-2").unwrap();
        assert_eq!(repo, "openai/gpt-2");
        assert_eq!(rev, None);
    }

    #[test]
    fn test_parse_hf_id_with_url() {
        let (repo, _) = parse_hf_id("https://huggingface.co/bert-base-uncased/bert").unwrap();
        assert_eq!(repo, "bert-base-uncased/bert");
    }

    #[test]
    fn test_parse_hf_id_invalid() {
        let result = parse_hf_id("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tfhub_url() {
        let (model, version) = parse_tfhub_url("https://tfhub.dev/google/imagenet/1").unwrap();
        assert_eq!(model, "google/imagenet");
        assert_eq!(version, "1");
    }

    #[test]
    fn test_parse_tfhub_url_without_https() {
        let (model, version) = parse_tfhub_url("tfhub.dev/google/bert/2").unwrap();
        assert_eq!(model, "google/bert");
        assert_eq!(version, "2");
    }

    #[test]
    fn test_parse_onnx_path() {
        let path = parse_onnx_path("vision/resnet50.onnx").unwrap();
        assert_eq!(path, "vision/resnet50.onnx");
    }

    #[test]
    fn test_parse_onnx_path_invalid() {
        let result = parse_onnx_path("invalid-path");
        assert!(result.is_err());
    }
}
