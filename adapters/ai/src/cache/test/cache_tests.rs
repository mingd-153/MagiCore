use super::*;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn test_cache_dir_huggingface() {
    let dir = cache_dir(&Registry::HuggingFace).unwrap();
    assert!(dir.to_string_lossy().contains("huggingface"));
}

#[test]
fn test_cache_dir_local() {
    let local = PathBuf::from("/tmp/models");
    let dir = cache_dir(&Registry::Local(local.clone())).unwrap();
    assert_eq!(dir, local);
}

#[test]
fn test_model_cache_path() {
    let path = model_cache_path(&Registry::HuggingFace, "openai/gpt-2").unwrap();
    assert!(path.to_string_lossy().contains("openai--gpt-2"));
}

#[test]
fn test_is_cached_false() {
    let tmp = tmp();
    let registry = Registry::Local(tmp.path().to_path_buf());
    let cached = is_cached(&registry, "missing-model").unwrap();
    assert!(!cached);
}

#[test]
fn test_is_cached_true() {
    let tmp = tmp();
    let model_dir = tmp.path().join("bert--base");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("model.bin"), b"fake").unwrap();

    let registry = Registry::Local(tmp.path().to_path_buf());
    let cached = is_cached(&registry, "bert/base").unwrap();
    assert!(cached);
}

#[test]
fn test_list_cached_models() {
    let tmp = tmp();
    std::fs::create_dir_all(tmp.path().join("model1--v1")).unwrap();
    std::fs::create_dir_all(tmp.path().join("model2--v2")).unwrap();

    let registry = Registry::Local(tmp.path().to_path_buf());
    let models = list_cached_models(&registry).unwrap();

    assert_eq!(models.len(), 2);
    assert!(models.contains(&"model1/v1".to_string()));
    assert!(models.contains(&"model2/v2".to_string()));
}

#[test]
fn test_cache_size() {
    let tmp = tmp();
    std::fs::write(tmp.path().join("model1.bin"), vec![0u8; 100]).unwrap();
    std::fs::write(tmp.path().join("model2.bin"), vec![0u8; 200]).unwrap();

    let registry = Registry::Local(tmp.path().to_path_buf());
    let size = cache_size(&registry).unwrap();

    assert_eq!(size, 300);
}
