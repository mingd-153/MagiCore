#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for command definitions (clap parsing 80+ commands)

use super::*;
use crate::Cli;
use clap::{CommandFactory, Parser};

#[test]
fn test_config_parses_get_set_delete_local() {
        let get = Cli::try_parse_from(["mgc", "config", "get", "registry"]).unwrap();
        match get.command.unwrap() {
            Commands::Config { cmd, local } => match cmd {
                crate::commands::config::ConfigCmd::Get { key } => {
                    assert_eq!(key, "registry");
                    assert!(!local);
                }
                _ => panic!("expected config get"),
            },
            _ => panic!("expected config command"),
        }

        let set = Cli::try_parse_from(["mgc", "config", "set", "x", "1", "--local"]).unwrap();
        match set.command.unwrap() {
            Commands::Config { cmd, local } => match cmd {
                crate::commands::config::ConfigCmd::Set { key, value, toml } => {
                    assert_eq!(key, "x");
                    assert_eq!(value, "1");
                    assert!(local);
                    assert!(!toml); // mặc định không phải toml
                }
                _ => panic!("expected config set"),
            },
            _ => panic!("expected config command"),
        }

        // mgc config set --toml
        let set_toml =
            Cli::try_parse_from(["mgc", "config", "set", "ecosystem", "web", "--toml"]).unwrap();
        match set_toml.command.unwrap() {
            Commands::Config { cmd, .. } => match cmd {
                crate::commands::config::ConfigCmd::Set { key, value, toml } => {
                    assert_eq!(key, "ecosystem");
                    assert_eq!(value, "web");
                    assert!(toml);
                }
                _ => panic!("expected config set --toml"),
            },
            _ => panic!("expected config command"),
        }

        // mgc config unset
        let unset = Cli::try_parse_from(["mgc", "config", "unset", "registry"]).unwrap();
        match unset.command.unwrap() {
            Commands::Config { cmd, .. } => match cmd {
                crate::commands::config::ConfigCmd::Unset { key, .. } => {
                    assert_eq!(key, "registry");
                }
                _ => panic!("expected config unset"),
            },
            _ => panic!("expected config command"),
        }

        // mgc config list --local
        let list = Cli::try_parse_from(["mgc", "config", "list", "--local"]).unwrap();
        match list.command.unwrap() {
            Commands::Config { cmd, .. } => match cmd {
                crate::commands::config::ConfigCmd::List { local } => {
                    assert!(local);
                }
                _ => panic!("expected config list"),
            },
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn test_stage_parses_dir_flag() {
        let cli = Cli::try_parse_from(["mgc", "stage", "--dir", "/tmp/demo"]).unwrap();
        match cli.command.unwrap() {
            Commands::Stage { dir } => {
                assert_eq!(dir.as_deref(), Some(std::path::Path::new("/tmp/demo")));
            }
            _ => panic!("expected stage command"),
        }
    }

    #[test]
    fn test_per_core_aliases_resolve() {
        let cmd = Cli::command();
        for (alias, expected) in [
            ("i-web", "install-web"),
            ("a-lib", "add-lib"),
            ("rm-ai", "remove-ai"),
            ("up-clo", "update-clo"),
            ("ls-hardware", "list-hardware"),
            ("i-hardware", "install-hardware"),
        ] {
            let found = cmd
                .find_subcommand(alias)
                .unwrap_or_else(|| panic!("alias {alias} not found"));
            assert_eq!(
                found.get_name(),
                expected,
                "alias {alias} should map to {expected}"
            );
        }
    }

    #[test]
    fn test_create_web_accepts_flags() {
        let cli = Cli::try_parse_from([
            "mgc",
            "create-web",
            "react@latest",
            "demo-app",
            "--ts",
            "--tailwindcss",
        ])
        .unwrap();

        match cli.command.unwrap() {
            Commands::CreateWeb {
                framework,
                project_name,
                flags,
            } => {
                assert_eq!(framework, "react@latest");
                assert_eq!(project_name, "demo-app");
                assert!(flags.ts);
                assert!(flags.tailwindcss);
            }
            _ => panic!("expected create-web command"),
        }
    }

    #[test]
    fn test_create_core_commands_follow_create_core_name_shape() {
        let ai = Cli::try_parse_from(["mgc", "create-ai", "python-agent", "demo-ai"]).unwrap();
        match ai.command.unwrap() {
            Commands::CreateAi {
                framework,
                project_name,
            } => {
                assert_eq!(framework, "python-agent");
                assert_eq!(project_name, "demo-ai");
            }
            _ => panic!("expected create-ai command"),
        }

        let app = Cli::try_parse_from(["mgc", "create-app", "flutter", "demo-app"]).unwrap();
        match app.command.unwrap() {
            Commands::CreateApp {
                framework,
                project_name,
            } => {
                assert_eq!(framework, "flutter");
                assert_eq!(project_name, "demo-app");
            }
            _ => panic!("expected create-app command"),
        }

        let lib = Cli::try_parse_from(["mgc", "create-lib", "demo-lib"]).unwrap();
        match lib.command.unwrap() {
            Commands::CreateLib { project_name } => assert_eq!(project_name, "demo-lib"),
            _ => panic!("expected create-lib command"),
        }
    }

    #[test]
    fn test_add_web_accepts_multiple_packages() {
        let cli =
            Cli::try_parse_from(["mgc", "add-web", "zod", "lodash", "@types/node", "-D"]).unwrap();

        match cli.command.unwrap() {
            Commands::AddWeb { packages, dev, .. } => {
                assert_eq!(packages, vec!["zod", "lodash", "@types/node"]);
                assert!(dev);
            }
            _ => panic!("expected add-web command"),
        }
    }

    #[test]
    fn test_global_quiet_flag_parses() {
        let cli = Cli::try_parse_from(["mgc", "--quiet", "add-web", "zod"]).unwrap();
        assert!(cli.quiet);
        match cli.command.unwrap() {
            Commands::AddWeb { packages, .. } => assert_eq!(packages, vec!["zod"]),
            _ => panic!("expected add-web command"),
        }
    }

    #[test]
    fn test_add_and_remove_accept_no_install() {
        let add = Cli::try_parse_from(["mgc", "add-web", "dayjs", "--no-install"]).unwrap();
        match add.command.unwrap() {
            Commands::AddWeb { no_install, .. } => assert!(no_install),
            _ => panic!("expected add-web command"),
        }

        let remove =
            Cli::try_parse_from(["mgc", "remove-web", "zod", "lodash", "--no-install"]).unwrap();
        match remove.command.unwrap() {
            Commands::RemoveWeb {
                packages,
                no_install,
            } => {
                assert_eq!(packages, vec!["zod", "lodash"]);
                assert!(no_install);
            }
            _ => panic!("expected remove-web command"),
        }
    }

    #[test]
    fn test_install_accepts_script_policy_flags() {
        let install = Cli::try_parse_from(["mgc", "install", "--allow-scripts"]).unwrap();
        match install.command.unwrap() {
            Commands::Install {
                ignore_scripts,
                allow_scripts,
                ..
            } => {
                assert!(!ignore_scripts);
                assert!(allow_scripts);
            }
            _ => panic!("expected install command"),
        }

        let install_web =
            Cli::try_parse_from(["mgc", "install-web", "--ignore-scripts", "--allow-scripts"])
                .unwrap();
        match install_web.command.unwrap() {
            Commands::InstallWeb {
                ignore_scripts,
                allow_scripts,
                ..
            } => {
                assert!(ignore_scripts);
                assert!(allow_scripts);
            }
            _ => panic!("expected install-web command"),
        }
    }

    #[test]
    fn test_install_accepts_package_specs() {
        let install = Cli::try_parse_from([
            "mgc",
            "install",
            "react@latest",
            "zod@^3.22.4",
            "--allow-scripts",
        ])
        .unwrap();

        match install.command.unwrap() {
            Commands::Install {
                packages,
                allow_scripts,
                ..
            } => {
                assert_eq!(packages, vec!["react@latest", "zod@^3.22.4"]);
                assert!(allow_scripts);
            }
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn test_cache_command_accepts_status_and_clean_targets() {
        let status = Cli::try_parse_from(["mgc", "cache", "status", "--target", "shared"]).unwrap();
        match status.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                ..
            } => {
                assert_eq!(action, "status");
                assert_eq!(target, "shared");
                assert!(!yes);
            }
            _ => panic!("expected cache command"),
        }

        let clean =
            Cli::try_parse_from(["mgc", "cache", "clean", "--target", "build", "--yes"]).unwrap();
        match clean.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                ..
            } => {
                assert_eq!(action, "clean");
                assert_eq!(target, "build");
                assert!(yes);
            }
            _ => panic!("expected cache command"),
        }

        let prune =
            Cli::try_parse_from(["mgc", "cache", "prune", "--target", "shared", "--yes"]).unwrap();
        match prune.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                dry_run,
                ..
            } => {
                assert_eq!(action, "prune");
                assert_eq!(target, "shared");
                assert!(yes);
                assert!(!dry_run);
            }
            _ => panic!("expected cache command"),
        }

        let dry_run =
            Cli::try_parse_from(["mgc", "cache", "prune", "--target", "shared", "--dry-run"])
                .unwrap();
        match dry_run.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                dry_run,
            } => {
                assert_eq!(action, "prune");
                assert_eq!(target, "shared");
                assert!(!yes);
                assert!(dry_run);
            }
            _ => panic!("expected cache command"),
        }
    }

    #[test]
    fn test_available_cores_matches_build_shape() {
        let available = crate::factory::available_cores();

        #[cfg(feature = "web")]
        assert!(available.iter().any(|(core, _)| *core == "web"));
        #[cfg(feature = "ai")]
        assert!(available.iter().any(|(core, _)| *core == "ai"));
        #[cfg(feature = "app")]
        assert!(available.iter().any(|(core, _)| *core == "app"));
        #[cfg(feature = "lib")]
        assert!(available.iter().any(|(core, _)| *core == "lib"));
        #[cfg(feature = "game")]
        assert!(available.iter().any(|(core, _)| *core == "game"));
        #[cfg(feature = "iot")]
        assert!(available.iter().any(|(core, _)| *core == "iot"));
        #[cfg(feature = "clo")]
        assert!(available.iter().any(|(core, _)| *core == "clo"));
        #[cfg(feature = "cicd")]
        assert!(available.iter().any(|(core, _)| *core == "cicd"));
        #[cfg(feature = "hardware")]
        assert!(available.iter().any(|(core, _)| *core == "hardware"));
    }

    #[test]
    fn test_help_surface_matches_build_shape() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("dev"));
        #[cfg(all(feature = "web", not(feature = "all")))]
        {
            assert!(help.contains("create"));
            assert!(help.contains("create-web"));
        }
        #[cfg(any(not(feature = "web"), feature = "all"))]
        {
            assert!(help.contains("install-web"));
            assert!(help.contains("create-web"));
            assert!(help.contains("add-web"));
        }

        for core in ["web", "ai", "app", "lib"] {
            assert!(
                help.contains(&format!("create-{core}")),
                "help must expose create-{core}"
            );
            assert!(
                help.contains(&format!("install-{core}")),
                "help must expose install-{core}"
            );
            assert!(
                help.contains(&format!("add-{core}")),
                "help must expose add-{core}"
            );
        }

        #[cfg(any(not(feature = "web"), feature = "all"))]
        assert!(!help.contains("create   "));
    }

    #[test]
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    fn test_single_core_create_alias_parses() {
        let cli =
            Cli::try_parse_from(["mgc", "create", "react@latest", "demo-app", "--ts"]).unwrap();

        match cli.command.unwrap() {
            Commands::CreateWeb {
                framework,
                project_name,
                flags,
            } => {
                assert_eq!(framework, "react@latest");
                assert_eq!(project_name, "demo-app");
                assert!(flags.ts);
            }
            _ => panic!("expected create-web command through single-core alias"),
        }
    }

    #[test]
    fn test_dev_command_accepts_host_and_port() {
        let cli =
            Cli::try_parse_from(["mgc", "dev", "--host", "127.0.0.1", "--port", "4315"]).unwrap();

        match cli.command.unwrap() {
            Commands::Dev {
                host,
                port,
                clear: _,
            } => {
                assert_eq!(host.as_deref(), Some("127.0.0.1"));
                assert_eq!(port, Some(4315));
            }
            _ => panic!("expected dev command"),
        }
    }

    #[test]
    fn test_deploy_defaults_to_dry_run() {
        let cli = Cli::try_parse_from(["mgc", "deploy"]).unwrap();
        match cli.command.unwrap() {
            Commands::Deploy { run } => assert!(!run),
            _ => panic!("expected deploy command"),
        }
        let cli = Cli::try_parse_from(["mgc", "deploy", "--run"]).unwrap();
        match cli.command.unwrap() {
            Commands::Deploy { run } => assert!(run),
            _ => panic!("expected deploy command"),
        }
    }

    #[test]
    fn test_install_parses_dry_run_flag() {
        let cli = Cli::try_parse_from(["mgc", "install", "--dry-run"]).unwrap();
        match cli.command.unwrap() {
            Commands::Install { dry_run, .. } => assert!(dry_run),
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn test_workspace_list_parses_filter_and_json() {
        let cli =
            Cli::try_parse_from(["mgc", "workspace", "list", "--filter", "./apps/*", "--json"])
                .unwrap();

        match cli.command.unwrap() {
            Commands::Workspace { cmd } => match cmd {
                crate::commands::workspace::WorkspaceCmd::List { filter, json } => {
                    assert_eq!(filter.as_deref(), Some("./apps/*"));
                    assert!(json);
                }
            },
            _ => panic!("expected workspace command"),
        }
    }
}
