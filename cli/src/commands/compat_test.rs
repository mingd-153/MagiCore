//! Compat-gate unit tests (native-engine architecture 2026-09-10).
//! Prove the contract BEFORE any spawn decision: native mode refuses
//! rival runtimes, compat mode allows only the opted-in runtime with
//! warning, external PMs are never spawnable, invalid flags fail closed.
//! Unit test cổng compat: native từ chối runtime đối thủ, compat chỉ cho
//! runtime đã chọn kèm cảnh báo, PM ngoài không bao giờ spawn được.

use crate::commands::compat::{CompatMode, gate_runtime_spawn};

#[test]
fn native_mode_refuses_bun_and_deno() {
    let mode = CompatMode::from_flag(None).unwrap();
    assert!(matches!(mode, CompatMode::Native));
    for runtime in ["bun", "deno"] {
        let err = gate_runtime_spawn(&mode, runtime).unwrap_err();
        assert!(
            err.to_string().contains("NATIVE"),
            "error must point at the native engine contract: {err}"
        );
    }
}

#[test]
fn compat_mode_allows_only_the_opted_in_runtime() {
    let mode = CompatMode::from_flag(Some("bun")).unwrap();
    assert!(gate_runtime_spawn(&mode, "bun").is_ok());
    let err = gate_runtime_spawn(&mode, "deno").unwrap_err();
    assert!(
        err.to_string().contains("--compat-runtime"),
        "error must name the escape hatch: {err}"
    );
}

#[test]
fn external_package_managers_never_spawn_even_in_compat() {
    for flag in [Some("bun"), Some("deno"), None] {
        let mode = CompatMode::from_flag(flag).unwrap();
        for pm in ["npm", "npx", "pnpm", "yarn", "bunx"] {
            let err = gate_runtime_spawn(&mode, pm).unwrap_err();
            assert!(
                err.to_string().contains("never spawnable"),
                "PM '{pm}' must be rejected under flag {flag:?}: {err}"
            );
        }
    }
}

#[test]
fn toolchain_binaries_pass_without_compat_optin() {
    let mode = CompatMode::from_flag(None).unwrap();
    for tool in ["cargo", "go", "pytest", "node", "flutter", "gradle"] {
        assert!(
            gate_runtime_spawn(&mode, tool).is_ok(),
            "toolchain binary '{tool}' must not require compat"
        );
    }
}

#[test]
fn invalid_runtime_flag_fails_closed() {
    for bad in ["node", "npm", "cargo", "python", ""] {
        assert!(
            CompatMode::from_flag(Some(bad)).is_err(),
            "invalid compat runtime '{bad}' must fail closed"
        );
    }
}
