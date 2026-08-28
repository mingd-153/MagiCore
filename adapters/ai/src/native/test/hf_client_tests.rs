#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Tests cho native/hf_client — tách khỏi src theo RULE §5. Parser test offline thuần.
// (Tests for native/hf_client — split per RULE §5; parser tests are fully offline.)

use super::*;

#[test]
fn test_client_builder() {
    let client = HuggingFaceClient::new()
        .with_token("hf_token123".into())
        .with_api_url("https://custom.api".into());

    assert_eq!(client.token, Some("hf_token123".to_string()));
    assert_eq!(client.api_url, "https://custom.api");
}

#[test]
fn test_client_default() {
    let client = HuggingFaceClient::default();
    assert_eq!(client.api_url, "https://huggingface.co/api");
    assert_eq!(client.token, None);
}

#[test]
fn test_parse_model_info_full() {
    let json: serde_json::Value = serde_json::from_str(
        r#"{
            "id": "bert-base-uncased",
            "author": "google-bert",
            "downloads": 4000000,
            "tags": ["transformers", "pytorch"],
            "pipeline_tag": "fill-mask"
        }"#,
    )
    .unwrap();

    let info = parse_model_info(&json, "fallback");
    assert_eq!(info.id, "bert-base-uncased");
    assert_eq!(info.author.as_deref(), Some("google-bert"));
    assert_eq!(info.downloads, Some(4_000_000));
    assert_eq!(info.tags.len(), 2);
    assert_eq!(info.pipeline_tag.as_deref(), Some("fill-mask"));
}

#[test]
fn test_parse_model_info_minimal_uses_fallback_id() {
    let json: serde_json::Value = serde_json::from_str("{}").unwrap();
    let info = parse_model_info(&json, "gpt2");
    assert_eq!(info.id, "gpt2", "thiếu id → dùng fallback, không panic");
    assert_eq!(info.author, None);
    assert!(info.tags.is_empty());
}

#[test]
fn test_parse_model_ids_skips_missing() {
    let json: serde_json::Value =
        serde_json::from_str(r#"[{"id":"a"},{"nope":1},{"id":"b"}]"#).unwrap();
    assert_eq!(
        parse_model_ids(&json),
        vec!["a".to_string(), "b".to_string()]
    );
    assert!(parse_model_ids(&serde_json::Value::Null).is_empty());
}

#[tokio::test]
async fn test_get_model_info_rejects_empty_id_offline() {
    // Fail-fast không cần mạng: id rỗng phải lỗi ngay
    // (Fail fast without network: empty id must error immediately)
    let client = HuggingFaceClient::new();
    assert!(client.get_model_info("").await.is_err());
    assert!(client.get_model_info("   ").await.is_err());
}

#[tokio::test]
async fn test_model_exists_rejects_empty_offline() {
    let client = HuggingFaceClient::new();
    assert!(
        !client.model_exists("").await.unwrap(),
        "id rỗng → false, không gọi mạng"
    );
}

// Network tests — chỉ chạy chủ động (hermetic CI bỏ qua).
// (Network tests — run manually only; hermetic CI skips them.)
#[tokio::test]
#[ignore = "hits huggingface.co — run manually"]
async fn test_get_model_info_live() {
    let client = HuggingFaceClient::new();
    let info = client.get_model_info("gpt2").await.unwrap();
    assert_eq!(info.id, "gpt2");
    assert!(info.author.is_some(), "live HF API returns an author field");
}

#[tokio::test]
#[ignore = "hits huggingface.co — run manually"]
async fn test_list_models_live() {
    let client = HuggingFaceClient::new();
    let models = client.list_models(Some(3)).await.unwrap();
    assert_eq!(models.len(), 3);
}
