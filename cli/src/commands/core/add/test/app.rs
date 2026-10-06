use super::*;
#[test]
fn flutter_native_specs_use_shared_dependency_syntax() {
    assert_eq!(
        flutter_native_specs(&["http".to_string(), "args:^1.2.0".to_string()], None).unwrap(),
        vec!["http", "args@^1.2.0"]
    );
    assert_eq!(
        flutter_native_specs(&["http args".to_string()], Some("^2.0.0")).unwrap(),
        vec!["http@^2.0.0", "args@^2.0.0"]
    );
}

#[test]
fn flutter_native_specs_reject_ambiguous_or_empty_constraints() {
    assert!(flutter_native_specs(&["http:^1.0.0".to_string()], Some("^2.0.0")).is_err());
    assert!(flutter_native_specs(&["http:".to_string()], None).is_err());
}
