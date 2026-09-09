//! Create-command project name security gate — path traversal regression.
//! Regression test cho lỗ hổng traversal tại CLI boundary (security stress
//! §7): mọi create-* phải chặn project name chứa separator/absolute/
//! parent-hop TRƯỚC khi chạm filesystem — không tạo directory rác.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use super::validate_project_name;

#[test]
fn test_traversal_project_name_fails_closed_at_boundary() {
    for evil in [
        "..",
        "../escape",
        "foo/../bar",
        "/tmp/absolute-attack",
        "C:\\evil",
        "\\\\?\\C:\\evil",
        "a/b",
        "a\\b",
        "~root",
        "",
        ".",
    ] {
        let err = validate_project_name(evil);
        assert!(err.is_err(), "name {evil:?} must be rejected");
        let msg = format!("{}", err.unwrap_err());
        assert!(
            msg.contains("Invalid project name"),
            "actionable error expected, got: {msg}"
        );
    }
}

#[test]
fn test_valid_project_names_pass_gate() {
    for ok_name in ["myApp", "dự-án-tiếng-việt", "proj_été", "app.name", "x"] {
        assert!(
            validate_project_name(ok_name).is_ok(),
            "valid name {ok_name:?} must pass"
        );
    }
}

#[test]
fn test_dot_and_hidden_names_do_not_escape_cwd() {
    // "." and names made only of dots resolve to cwd ancestors — reject.
    // "." và chuỗi dot trỏ tới cwd/cha — chặn hết.
    for evil in [".", "...", "...."] {
        assert!(
            validate_project_name(evil).is_err(),
            "dot-name {evil:?} must be rejected"
        );
    }
}
