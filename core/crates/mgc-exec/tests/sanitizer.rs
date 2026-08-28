#![allow(clippy::unwrap_used)]
//! Sanitizer tests — args nhạy cảm REDACTED trước khi vào audit log (00-index §5.4)
//! (A13: token/password/key/authToken → [REDACTED]; secret không bao giờ vào log)

use mgc_exec::prelude::redact_args;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn redacts_equals_value() {
    let a = redact_args(&s(&["--token=abc123", "--path=./src"]));
    assert_eq!(a[0], "--token=[REDACTED]");
    assert_eq!(a[1], "--path=./src");
}

#[test]
fn redacts_separate_value() {
    let a = redact_args(&s(&["--token", "supersecret", "-v"]));
    assert_eq!(a, s(&["--token", "[REDACTED]", "-v"]));
}

#[test]
fn redacts_short_p_password() {
    let a = redact_args(&s(&["-p", "hunter2"]));
    assert_eq!(a, s(&["-p", "[REDACTED]"]));
}

#[test]
fn redacts_key_and_auth_token() {
    let a = redact_args(&s(&["--api-key=K", "_authToken=abc", "--secret=x"]));
    assert_eq!(
        a,
        s(&[
            "--api-key=[REDACTED]",
            "_authToken=[REDACTED]",
            "--secret=[REDACTED]"
        ])
    );
}

#[test]
fn keeps_plain_args() {
    let a = redact_args(&s(&["--no-scripts", "build", "src/main.rs"]));
    assert_eq!(a, s(&["--no-scripts", "build", "src/main.rs"]));
}

#[test]
fn dangling_sensitive_flag_gets_redacted() {
    let a = redact_args(&s(&["--token"]));
    assert_eq!(a, s(&["--token", "[REDACTED]"]));
}

#[test]
fn empty_args_pass_through() {
    assert_eq!(redact_args(&[]), Vec::<String>::new());
}
