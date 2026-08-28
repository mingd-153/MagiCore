#![allow(clippy::unwrap_used)]

use mgc_lockfile::{serialization, Lockfile, Package};

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

    assert_eq!(decoded.version, "2");
    assert_eq!(decoded.packages[0].dependencies, ["scheduler@0.25.0"]);
}
