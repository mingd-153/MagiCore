use super::*;
use crate::LibLanguage;
use std::fs;

#[test]
fn python_age_policy_uses_the_adapter_core_override() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("mgc.toml"),
        "[security]\nmin_release_age = 72\nlib = 8\nai = 13\nallow_missing_time = true\n",
    )
    .unwrap();

    let lib = adapter_for_language(LibLanguage::Python, project.path(), None, None).unwrap();
    let ai = adapter_for_ai_python(project.path()).unwrap();

    assert_eq!(
        lib.security_age_gate_policy_for(project.path()).unwrap(),
        Some((8, true))
    );
    assert_eq!(
        ai.security_age_gate_policy_for(project.path()).unwrap(),
        Some((13, true))
    );
}

#[test]
fn python_age_policy_uses_global_fallback_and_rejects_invalid_config() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("mgc.toml"),
        "[security]\nmin_release_age = 36\n",
    )
    .unwrap();
    let ai = adapter_for_ai_python(project.path()).unwrap();
    assert_eq!(
        ai.security_age_gate_policy_for(project.path()).unwrap(),
        Some((36, false))
    );

    fs::write(
        project.path().join("mgc.toml"),
        "[security]\nmin_release_age = \"36\"\n",
    )
    .unwrap();
    assert!(ai.security_age_gate_policy_for(project.path()).is_err());
}

#[test]
fn unsupported_native_lib_lanes_reject_an_explicit_age_gate() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("mgc.toml"), "[security]\nlib = 24\n").unwrap();
    let rust = adapter_for_language(LibLanguage::Rust, project.path(), None, None).unwrap();

    assert!(rust.arm_age_gate_for(project.path()).is_err());
}
