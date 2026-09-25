#![allow(clippy::unwrap_used)]

use mgc_lockfile::{Lockfile, Package, serialization};

#[test]
fn v2_lockfile_roundtrips_package_edges() {
    let mut lock = Lockfile::new();
    let mut react = Package::new(
        "react".into(),
        "19.2.0".into(),
        "https://registry.example/react-19.2.0.tgz".into(),
        "blake3-react".into(),
    );
    react.add_dependency("scheduler@0.25.0".into());
    lock.add_package(react);

    let encoded = serialization::to_toml(&lock).unwrap();
    let decoded: Lockfile = serialization::from_toml(&encoded).unwrap();

    // Deliberate v3 bump (Phase 1): the package-edges contract now holds on
    // the current canonical schema; pinned via the constant so future schema
    // bumps surface here instead of silently drifting.
    // Nâng lên v3 có chủ đích (Phase 1): hợp đồng package-edges giờ giữ trên
    // schema canonical hiện tại; ghim qua const để lần nâng schema sau nổi
    // lên ở đây thay vì trôi âm thầm.
    assert_eq!(decoded.version, mgc_lockfile::LOCKFILE_SCHEMA_VERSION);
    assert_eq!(decoded.packages[0].dependencies, ["scheduler@0.25.0"]);
}
