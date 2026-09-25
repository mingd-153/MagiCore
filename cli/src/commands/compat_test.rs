//! Compat-gate unit tests (native-engine architecture 2026-09-10).
//! Prove the contract BEFORE any spawn decision: native mode refuses
//! rival runtimes in every mode, external PMs are never spawnable, and
//! invalid flags fail closed. Unit test cổng runtime: luôn từ chối runtime
//! đối thủ và PM ngoài; giá trị compat cũ không mở đường spawn.

use crate::commands::compat::{CompatMode, gate_runtime_spawn};

#[test]
fn native_mode_refuses_bun_and_deno() {
    let mode = CompatMode::from_flag(None).unwrap();
    assert!(matches!(mode, CompatMode::Native));
    for runtime in ["bun", "deno"] {
        let err = gate_runtime_spawn(&mode, runtime).unwrap_err();
        assert!(
            err.to_string().contains("not implemented")
                && err.to_string().contains("no external runtime was invoked"),
            "error must state native support is absent and delegation did not run: {err}"
        );
    }
}

#[test]
fn compat_mode_does_not_enable_a_rival_runtime() {
    let mode = CompatMode::from_flag(Some("bun")).unwrap();
    for runtime in ["bun", "deno"] {
        let err = gate_runtime_spawn(&mode, runtime).unwrap_err();
        assert!(
            err.to_string().contains("not implemented")
                && err.to_string().contains("no external runtime was invoked"),
            "compat flags must not delegate to {runtime}: {err}"
        );
    }
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
