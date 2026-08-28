#![cfg(test)]
#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn test_model_info_builder() {
    let info = ModelInfo::new("gpt2");
    assert_eq!(info.id, "gpt2");
    assert_eq!(info.tags.len(), 0);
}
