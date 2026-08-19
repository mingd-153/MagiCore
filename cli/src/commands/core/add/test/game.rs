use super::*;

#[test]
fn game_split_routes_optimizer_apart() {
    let (adapter, has) = game_split(&["serde".to_string(), OPTIMIZER_PKG.to_string()]);
    assert!(has);
    assert_eq!(adapter, vec!["serde"]);
}

#[test]
fn game_split_all_optimizer_yields_empty_adapter() {
    let (adapter, has) = game_split(&[OPTIMIZER_PKG.to_string()]);
    assert!(has);
    assert!(adapter.is_empty());
}

#[test]
fn game_split_no_optimizer() {
    let (adapter, has) = game_split(&["tokio".to_string(), "axum".to_string()]);
    assert!(!has);
    assert_eq!(adapter.len(), 2);
}