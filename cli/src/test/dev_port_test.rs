#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for dev server port allocation

use super::*;

#[test]
fn all_default_ports_are_unique() {
    let ports = [
        PORT_WEB_FE,
        PORT_WEB_BE,
        PORT_AI,
        PORT_CLO,
        PORT_CICD,
        PORT_GAME,
        PORT_IOT,
        PORT_APP,
        PORT_LIB,
    ];
    let mut seen = std::collections::HashSet::new();
    for p in &ports {
        assert!(seen.insert(*p), "duplicate port {p} in dev_port table");
    }
}

#[test]
fn default_port_returns_correct_values() {
    assert_eq!(default_port("web"), Some(PORT_WEB_FE));
    assert_eq!(default_port("ai"), Some(PORT_AI));
    assert_eq!(default_port("clo"), Some(PORT_CLO));
    assert_eq!(default_port("cloud"), Some(PORT_CLO)); // alias
    assert_eq!(default_port("cicd"), Some(PORT_CICD));
    assert_eq!(default_port("game"), Some(PORT_GAME));
    assert_eq!(default_port("iot"), Some(PORT_IOT));
    assert_eq!(default_port("app"), Some(PORT_APP));
    assert_eq!(default_port("lib"), Some(PORT_LIB));
    assert_eq!(default_port("library"), Some(PORT_LIB)); // alias
    assert_eq!(default_port("hardware"), None); // hardware không có server
    assert_eq!(default_port("unknown"), None);
}

#[test]
fn check_multi_core_conflicts_detects_same_port() {
    // Hai core khác nhau nhưng override cùng port → conflict
    let cores = [("web", Some(9999u16)), ("ai", Some(9999u16))];
    let conflicts = check_multi_core_conflicts(&cores);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].2, 9999);
}

#[test]
fn check_multi_core_conflicts_no_conflict_on_unique_ports() {
    // Tất cả port khác nhau → không conflict
    let cores = [
        ("web", Some(4315u16)),
        ("ai", Some(5134u16)),
        ("game", Some(4351u16)),
    ];
    let conflicts = check_multi_core_conflicts(&cores);
    assert!(conflicts.is_empty());
}

#[test]
fn all_port_values_are_valid_permutations() {
    // Kiểm tra mỗi port chứa chỉ chữ số từ {1, 3, 4, 5}
    // (web BE = 3415 và web FE = 4315 đã user chốt; các core khác cũng dùng {1,3,4,5})
    let ports = [
        PORT_WEB_FE,
        PORT_WEB_BE,
        PORT_AI,
        PORT_CLO,
        PORT_CICD,
        PORT_GAME,
        PORT_IOT,
        PORT_APP,
        PORT_LIB,
    ];
    let valid_digits: std::collections::HashSet<char> = "1345".chars().collect();
    for p in &ports {
        for ch in p.to_string().chars() {
            assert!(
                valid_digits.contains(&ch),
                "port {p} has digit '{ch}' outside {{1,3,4,5}}"
            );
        }
    }
}
