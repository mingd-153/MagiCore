#![allow(clippy::unwrap_used)]

#[path = "../src/commands/start_config.rs"]
#[allow(dead_code)]
mod start_config;

#[test]
fn defaults_to_localhost_4315() {
    let (host, port) = start_config::resolve_web_start_bind_from_env(None, None).unwrap();
    assert_eq!(host, "localhost");
    assert_eq!(port, 4315);
}

#[test]
fn rejects_bind_all_hosts() {
    let err = start_config::resolve_web_start_bind_from_env(Some("0.0.0.0"), None).unwrap_err();
    assert!(err.to_string().contains("cannot bind all interfaces"));

    let err = start_config::resolve_web_start_bind_from_env(Some("::"), None).unwrap_err();
    assert!(err.to_string().contains("cannot bind all interfaces"));
}

#[test]
fn rejects_ports_outside_rule_set() {
    let err =
        start_config::resolve_web_start_bind_from_env(Some("localhost"), Some("3000")).unwrap_err();
    assert!(err.to_string().contains("approved MegaGate ports"));
}

#[test]
fn accepts_all_rule_ports() {
    for port in start_config::VALID_RULE_PORTS {
        let (host, resolved) = start_config::resolve_web_start_bind_from_env(
            Some("localhost"),
            Some(&port.to_string()),
        )
        .unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(resolved, port);
    }
}

#[test]
fn rejects_empty_host_and_invalid_port() {
    let err = start_config::resolve_web_start_bind_from_env(Some("  "), None).unwrap_err();
    assert!(err.to_string().contains("cannot be empty"));

    let err =
        start_config::resolve_web_start_bind_from_env(Some("localhost"), Some("abc")).unwrap_err();
    assert!(err.to_string().contains("must be a TCP port"));
}
