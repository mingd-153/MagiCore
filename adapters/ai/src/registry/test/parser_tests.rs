#![cfg(test)]
#![allow(clippy::unwrap_used)]

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
