#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for dispatch engine recursive and audit-strict logic

use super::*;

#[test]
fn audit_strict_rejects_materializing_install_commands() {
    let install = Commands::Install {
        packages: vec![],
        frozen: false,
        ignore_scripts: false,
        allow_scripts: false,
        prefer_dedupe: false,
        repair: false,
        dry_run: false,
        offline: false,
        compat_runtime: None,
    };
    assert!(reject_unsupported_audit_strict(&install).is_ok());

    let add = Commands::AddWeb {
        packages: vec!["zod".into()],
        dev: false,
        exact: false,
        optional: false,
        peer: false,
        no_save: false,
        no_install: false,
        global: false,
        compat_runtime: None,
        version: None,
    };
    assert!(reject_unsupported_audit_strict(&add).is_ok());
}

#[test]
fn audit_strict_allows_audit_and_manifest_only_mutation() {
    assert!(
        reject_unsupported_audit_strict(&Commands::Audit {
            cmd: None,
            fix: false,
            format: None
        })
        .is_ok()
    );

    let add = Commands::AddWeb {
        packages: vec!["zod".into()],
        dev: false,
        exact: false,
        optional: false,
        peer: false,
        no_save: false,
        no_install: true,
        global: false,
        compat_runtime: None,
        version: None,
    };
    assert!(reject_unsupported_audit_strict(&add).is_ok());
}

#[test]
fn audit_strict_rejects_commands_that_never_consult_strictness() {
    // Build/Remove never read MGC_AUDIT_STRICT — accepting the flag would
    // silently ignore it. Verify stays allowed (run_strict chain).
    let err = reject_unsupported_audit_strict(&Commands::Build {
        target: None,
        compat_runtime: None,
    })
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("--audit-strict has no effect on 'build'"),
        "unexpected error: {err}"
    );
    let err = reject_unsupported_audit_strict(&Commands::RemoveWeb {
        packages: vec![],
        no_install: false,
        compat_runtime: None,
    })
    .unwrap_err();
    assert!(
        err.to_string().contains("--audit-strict has no effect"),
        "unexpected error: {err}"
    );
    assert!(reject_unsupported_audit_strict(&Commands::Verify).is_ok());
    assert!(
        reject_unsupported_audit_strict(&Commands::RemoveHardware {
            packages: vec![],
            compat_runtime: None,
        })
        .is_err()
    );
}

#[test]
fn recursive_is_rejected_for_unsupported_commands() {
    // Init không có trong recursive_supported → phải bị reject.
    let err = reject_unsupported_recursive(Some(&Commands::Init {
        template: None,
        signature: None,
    }))
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("--recursive is not implemented for 'init' yet"),
        "unexpected error: {err}"
    );
}

#[test]
fn recursive_supported_includes_build_run_audit_outdated_dev() {
    // T4: xác nhận các lệnh mới được mở rộng recursive support
    assert!(recursive_supported(&Commands::Build {
        target: None,
        compat_runtime: None,
    }));
    assert!(recursive_supported(&Commands::Audit {
        cmd: None,
        fix: false,
        format: None
    }));
    assert!(recursive_supported(&Commands::Outdated { json: false }));
    assert!(recursive_supported(&Commands::Dev {
        host: None,
        port: None,
        clear: false,
        compat_runtime: None,
    }));
}

#[test]
fn recursive_supported_includes_install_and_add() {
    // Existing commands vẫn supported sau T4
    assert!(recursive_supported(&Commands::Install {
        packages: vec![],
        frozen: false,
        ignore_scripts: false,
        allow_scripts: false,
        prefer_dedupe: false,
        repair: false,
        dry_run: false,
        offline: false,
        compat_runtime: None,
    }));
    assert!(recursive_supported(&Commands::List {
        compat_runtime: None
    }));
}

#[test]
fn recursive_core_selector_cannot_override_marker_or_project_config() {
    let workspace = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::new("owned-web", "web")
        .save(workspace.path())
        .unwrap();

    let error = resolve_recursive_workspace_core(workspace.path(), Some("ai")).unwrap_err();

    assert!(error.to_string().contains("conflicts with workspace"));
    assert_eq!(
        resolve_recursive_workspace_core(workspace.path(), Some("web")).unwrap(),
        Some("web".to_string())
    );

    std::fs::remove_file(workspace.path().join(".mgc.core")).unwrap();
    let error = resolve_recursive_workspace_core(workspace.path(), Some("ai")).unwrap_err();
    assert!(error.to_string().contains("conflicts with workspace"));
}

#[test]
fn recursive_core_selector_can_select_only_an_unclaimed_workspace() {
    let workspace = tempfile::tempdir().unwrap();

    assert_eq!(
        resolve_recursive_workspace_core(workspace.path(), Some("ai")).unwrap(),
        Some("ai".to_string())
    );
    assert_eq!(
        resolve_recursive_workspace_core(workspace.path(), None).unwrap(),
        None
    );
}

#[test]
fn explicit_core_dependency_command_must_match_workspace_identity_and_global_selector() {
    let workspace = tempfile::tempdir().unwrap();
    mgc_config::project::ProjectConfig::new("owned-web", "web")
        .save(workspace.path())
        .unwrap();

    assert!(validate_explicit_dependency_core(workspace.path(), "web", None).is_ok());
    assert!(validate_explicit_dependency_core(workspace.path(), "web", Some("web")).is_ok());

    let owner_error = validate_explicit_dependency_core(workspace.path(), "app", None).unwrap_err();
    assert!(owner_error.to_string().contains("belongs to core 'web'"));

    let selector_error =
        validate_explicit_dependency_core(workspace.path(), "web", Some("app")).unwrap_err();
    assert!(
        selector_error
            .to_string()
            .contains("conflicts with the explicit 'web'")
    );
}

#[test]
fn explicit_dependency_command_mapping_covers_every_public_route() {
    use clap::Parser;

    let cases = [
        ("install-web", "web", false),
        ("install-game", "game", false),
        ("install-ai", "ai", false),
        ("install-clo", "cloud", false),
        ("install-cicd", "cicd", false),
        ("install-iot", "iot", false),
        ("install-app", "app", false),
        ("install-lib", "lib", false),
        ("install-hardware", "hardware", false),
        ("add-web", "web", true),
        ("add-game", "game", true),
        ("add-ai", "ai", true),
        ("add-clo", "cloud", true),
        ("add-cicd", "cicd", true),
        ("add-iot", "iot", true),
        ("add-app", "app", true),
        ("add-lib", "lib", true),
        ("add-hardware", "hardware", true),
        ("remove-web", "web", true),
        ("remove-game", "game", true),
        ("remove-ai", "ai", true),
        ("remove-clo", "cloud", true),
        ("remove-cicd", "cicd", true),
        ("remove-iot", "iot", true),
        ("remove-app", "app", true),
        ("remove-lib", "lib", true),
        ("list-web", "web", false),
        ("list-game", "game", false),
        ("list-ai", "ai", false),
        ("list-clo", "cloud", false),
        ("list-cicd", "cicd", false),
        ("list-iot", "iot", false),
        ("list-app", "app", false),
        ("list-lib", "lib", false),
        ("list-hardware", "hardware", false),
        ("update-web", "web", false),
        ("update-game", "game", false),
        ("update-ai", "ai", false),
        ("update-clo", "cloud", false),
        ("update-cicd", "cicd", false),
        ("update-iot", "iot", false),
        ("update-app", "app", false),
        ("update-lib", "lib", false),
    ];

    for (name, expected_core, needs_package) in cases {
        let mut args = vec!["mgc", name];
        if needs_package {
            args.push("fixture-package");
        }
        let cli = Cli::try_parse_from(args)
            .unwrap_or_else(|error| panic!("public command '{name}' did not parse: {error}"));
        let command = cli
            .command
            .unwrap_or_else(|| panic!("public command '{name}' produced no command"));
        assert_eq!(
            explicit_dependency_core(&command),
            Some(expected_core),
            "public command '{name}' must be bound to core '{expected_core}'"
        );
    }

    let cli = Cli::try_parse_from(["mgc", "build"]).unwrap();
    assert_eq!(explicit_dependency_core(&cli.command.unwrap()), None);
}

#[test]
fn bare_add_version_pin_reaches_core_command() {
    use crate::dispatch::bare::bare_core_command;
    use crate::dispatch::types::DispatchCommand;
    let command = Commands::Add {
        packages: vec!["zod".into()],
        version: Some("3.22.4".into()),
        dev: false,
        global: false,
        exact: false,
        optional: false,
        peer: false,
        no_save: false,
        no_install: false,
        compat_runtime: None,
    };
    match bare_core_command(command, Some("ai".to_string())).unwrap() {
        DispatchCommand::Core(crate::dispatch::types::CoreCommand::AddAi { version, .. }) => {
            assert_eq!(version.as_deref(), Some("3.22.4"))
        }
        _ => panic!("expected AddAi core command"),
    }
}

#[test]
fn per_core_add_version_pin_reaches_lane() {
    // T0.3-version parity: per-core `add-* --version` forwards the pin
    // (previously only bare `add` even parsed it, then dropped it).
    use crate::dispatch::per_core::command_to_dispatch;
    use crate::dispatch::types::DispatchCommand;
    let command = Commands::AddLib {
        packages: vec!["serde".into()],
        dev: false,
        exact: false,
        optional: false,
        peer: false,
        no_save: false,
        global: false,
        compat_runtime: None,
        version: Some("1.0.0".into()),
    };
    match command_to_dispatch(command, None).unwrap() {
        DispatchCommand::Core(crate::dispatch::types::CoreCommand::AddLib { version, .. }) => {
            assert_eq!(version.as_deref(), Some("1.0.0"))
        }
        _ => panic!("expected AddLib core command"),
    }
}

#[test]
fn shared_core_marker_reader_parses_plain_and_comment() {
    // T4: đọc .mgc.core marker với/không có comment
    let dir = std::env::temp_dir().join(format!("mgc_test_marker_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // Plain value
    std::fs::write(dir.join(".mgc.core"), "web\n").unwrap();
    assert_eq!(
        mgc_config::project::ProjectConfig::read_core_marker(&dir).unwrap(),
        Some("web".to_string())
    );

    // With comment
    std::fs::write(dir.join(".mgc.core"), "ai # generated by mgc init\n").unwrap();
    assert_eq!(
        mgc_config::project::ProjectConfig::read_core_marker(&dir).unwrap(),
        Some("ai".to_string())
    );

    // Comment-only marker is present but invalid, so identity must fail closed.
    std::fs::write(dir.join(".mgc.core"), "# just a comment\n").unwrap();
    assert!(mgc_config::project::ProjectConfig::read_core_marker(&dir).is_err());

    // Missing file → None
    std::fs::remove_file(dir.join(".mgc.core")).unwrap();
    assert_eq!(
        mgc_config::project::ProjectConfig::read_core_marker(&dir).unwrap(),
        None
    );

    std::fs::remove_dir(&dir).ok();
}
