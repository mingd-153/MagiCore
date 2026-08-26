//! Tests cho ai/registry — tách khỏi src theo RULE §5. Offline trừ test live có ignore.
// (Tests for ai/registry — split per RULE §5. Offline except the ignored live test.)

use super::*;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn test_registry_from_url() {
    assert_eq!(
        Registry::from_url("https://huggingface.co/bert-base"),
        Some(Registry::HuggingFace)
    );
    assert_eq!(
        Registry::from_url("https://tfhub.dev/google/model/1"),
        Some(Registry::TensorFlowHub)
    );
    assert_eq!(
        Registry::from_url("https://github.com/onnx/models"),
        Some(Registry::OnnxZoo)
    );
    assert_eq!(
        Registry::from_url("file:///models"),
        Some(Registry::Local(PathBuf::from("/models")))
    );
}

#[test]
fn test_registry_base_url() {
    assert_eq!(
        Registry::HuggingFace.base_url(),
        Some("https://huggingface.co")
    );
    assert_eq!(
        Registry::TensorFlowHub.base_url(),
        Some("https://tfhub.dev")
    );
    assert_eq!(Registry::Local(PathBuf::from("/tmp")).base_url(), None);
}

#[test]
fn test_registry_name() {
    assert_eq!(Registry::HuggingFace.name(), "huggingface");
    assert_eq!(Registry::TensorFlowHub.name(), "tfhub");
    assert_eq!(Registry::OnnxZoo.name(), "onnx-zoo");
}

#[test]
fn test_metadata_from_hf_json_offline() {
    let json: serde_json::Value = serde_json::from_str(
        r#"{
            "id": "gpt2",
            "sha": "e7da7f2",
            "tags": ["transformers", "text-generation"]
        }"#,
    )
    .unwrap();

    let meta = metadata_from_hf_json(&json, "fallback");
    assert_eq!(meta.id, "gpt2");
    assert_eq!(meta.registry, Registry::HuggingFace);
    assert_eq!(
        meta.version.as_deref(),
        Some("e7da7f2"),
        "'sha' → version revision"
    );
    assert_eq!(meta.tags.len(), 2);
    assert_eq!(meta.format, Some(ModelFormat::HuggingFace));
    assert_ne!(meta.tags, vec!["stub".to_string()], "không còn tag stub");
}

#[test]
fn test_metadata_from_hf_json_minimal_defaults_main() {
    let meta = metadata_from_hf_json(&serde_json::Value::Null, "mymodel");
    assert_eq!(meta.id, "mymodel", "thiếu id → fallback");
    assert_eq!(meta.version.as_deref(), Some("main"));
    assert!(meta.tags.is_empty());
}

#[tokio::test]
async fn test_query_local() {
    let tmp = tmp();
    let model = tmp.path().join("model.onnx");
    std::fs::write(&model, b"fake").unwrap();

    let meta = query_local(tmp.path(), "model.onnx").await.unwrap();
    assert_eq!(meta.id, "model.onnx");
    assert_eq!(meta.format, Some(ModelFormat::Onnx));
    assert_eq!(meta.size_bytes, Some(4));
}

#[test]
fn test_detect_format() {
    assert_eq!(
        detect_format(&PathBuf::from("model.safetensors")),
        Some(ModelFormat::SafeTensors)
    );
    assert_eq!(
        detect_format(&PathBuf::from("model.onnx")),
        Some(ModelFormat::Onnx)
    );
    assert_eq!(
        detect_format(&PathBuf::from("model.pt")),
        Some(ModelFormat::PyTorch)
    );
    assert_eq!(
        detect_format(&PathBuf::from("model.pb")),
        Some(ModelFormat::TensorFlow)
    );
}

// Network test — chỉ chạy chủ động (hermetic CI bỏ qua).
// (Network test — run manually only; hermetic CI skips it.)
#[tokio::test]
#[ignore = "hits huggingface.co — run manually"]
async fn test_query_huggingface_live() {
    let meta = query_huggingface("gpt2").await.unwrap();
    assert_eq!(meta.id, "gpt2");
    assert_eq!(meta.registry, Registry::HuggingFace);
}
