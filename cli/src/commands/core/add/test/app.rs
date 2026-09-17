use super::*;
use mgc_app_adapter::AppLanguage;

#[test]
fn flutter_version_pins_with_pub_constraint() {
    // Verified against the Dart pub reference: `dart pub add` (and
    // therefore `flutter pub add`) accepts `pkg:constraint`.
    let out = apply_version_pin(
        AppLanguage::Flutter,
        &["http".to_string(), "args".to_string()],
        Some("^1.2.0"),
    )
    .unwrap();
    assert_eq!(out, vec!["http:^1.2.0", "args:^1.2.0"]);
}

#[test]
fn flutter_existing_constraint_plus_flag_fails_loudly() {
    let err = apply_version_pin(
        AppLanguage::Flutter,
        &["http:^1.0.0".to_string()],
        Some("^1.2.0"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("already carries"), "{err}");
}

#[test]
fn non_flutter_languages_reject_version_pin() {
    // Kotlin/Swift/ObjC lanes have no verified pin syntax — loud reject,
    // never silent drop.
    for lang in [AppLanguage::Kotlin, AppLanguage::Swift, AppLanguage::ObjC] {
        let err = apply_version_pin(lang, &["pkg".to_string()], Some("1.0.0")).unwrap_err();
        assert!(err.to_string().contains("--version"), "{err}");
    }
}

#[test]
fn no_pin_passes_packages_through() {
    let out = apply_version_pin(
        AppLanguage::Flutter,
        &["a b".to_string(), "c".to_string()],
        None,
    )
    .unwrap();
    assert_eq!(out, vec!["a", "b", "c"]);
}
