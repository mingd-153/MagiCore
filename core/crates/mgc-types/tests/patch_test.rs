use mgc_types::package::VersionRange;
use mgc_types::patch::{LockPatch, PatchKind, PatchSpec};

#[test]
fn patch_spec_serializes() {
    let vr = VersionRange::parse("^1.0.0").unwrap();
    let spec = PatchSpec::new(
        "react".into(),
        vr,
        "patches/react.patch".into(),
        "sha256-abc".into(),
    );
    let json = serde_json::to_string(&spec).unwrap();
    assert!(json.contains("react"));
    assert!(json.contains("sha256-abc"));
    let back: PatchSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(back.kind, PatchKind::Diff);
}

#[test]
fn lock_patch_serializes() {
    let lp = LockPatch::new(
        "react".into(),
        "1.0.0".into(),
        "sha256-def".into(),
        "2026-01-01T00:00:00Z".into(),
    );
    let json = serde_json::to_string(&lp).unwrap();
    let back: LockPatch = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "react");
    assert_eq!(back.sha256, "sha256-def");
}
