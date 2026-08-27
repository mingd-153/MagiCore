use mgc_types::ecosystem::Ecosystem;

#[test]
fn from_str_all_variants() {
    assert_eq!(Ecosystem::from_str("web"), Some(Ecosystem::Web));
    assert_eq!(Ecosystem::from_str("game"), Some(Ecosystem::Game));
    assert_eq!(Ecosystem::from_str("ai"), Some(Ecosystem::Ai));
    assert_eq!(Ecosystem::from_str("cloud"), Some(Ecosystem::Cloud));
    assert_eq!(Ecosystem::from_str("cicd"), Some(Ecosystem::Cicd));
    assert_eq!(Ecosystem::from_str("iot"), Some(Ecosystem::Iot));
    assert_eq!(Ecosystem::from_str("app"), Some(Ecosystem::App));
    assert_eq!(Ecosystem::from_str("lib"), Some(Ecosystem::Lib));
    assert_eq!(Ecosystem::from_str("hardware"), Some(Ecosystem::Hardware));
}

#[test]
fn from_str_cloud_alias() {
    assert_eq!(Ecosystem::from_str("clo"), Some(Ecosystem::Cloud));
}

#[test]
fn from_str_unknown_returns_none() {
    assert_eq!(Ecosystem::from_str("unknown"), None);
    assert_eq!(Ecosystem::from_str(""), None);
}

#[test]
fn as_str_all_variants() {
    assert_eq!(Ecosystem::Web.as_str(), "web");
    assert_eq!(Ecosystem::Game.as_str(), "game");
    assert_eq!(Ecosystem::Ai.as_str(), "ai");
    assert_eq!(Ecosystem::Cloud.as_str(), "cloud");
    assert_eq!(Ecosystem::Cicd.as_str(), "cicd");
    assert_eq!(Ecosystem::Iot.as_str(), "iot");
    assert_eq!(Ecosystem::App.as_str(), "app");
    assert_eq!(Ecosystem::Lib.as_str(), "lib");
    assert_eq!(Ecosystem::Hardware.as_str(), "hardware");
}

#[test]
fn roundtrip_all_variants() {
    for variant in [
        Ecosystem::Web,
        Ecosystem::Game,
        Ecosystem::Ai,
        Ecosystem::Cloud,
        Ecosystem::Cicd,
        Ecosystem::Iot,
        Ecosystem::App,
        Ecosystem::Lib,
        Ecosystem::Hardware,
    ] {
        let s = variant.as_str();
        assert_eq!(Ecosystem::from_str(s), Some(variant));
    }
}

#[test]
fn debug_format() {
    let s = format!("{:?}", Ecosystem::Web);
    assert_eq!(s, "Web");
}

#[test]
fn copy_works() {
    let a = Ecosystem::Ai;
    let b = a;
    assert_eq!(a, b);
}
