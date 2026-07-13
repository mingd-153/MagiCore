use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use mg_types::adapter::PackageAdapter;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::commands::core::scaffold_flags::ScaffoldFlags;
use crate::commands::core::shared;
use mg_ui::info;

const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";
const SCAFFOLD_VERSION_OVERRIDES_ENV: &str = "MEGAGATE_WEB_SCAFFOLD_VERSION_OVERRIDES";
const SCAFFOLD_BASELINE_VERSIONS: &[(&str, &str)] = &[
    ("vue", "^3.5.39"),
    ("react", "^19.2.7"),
    ("react-dom", "^19.2.7"),
    ("vite", "^8.1.4"),
    ("@vitejs/plugin-react", "^6.0.3"),
    ("@vitejs/plugin-vue", "^6.0.7"),
    ("solid-js", "^1.9.14"),
    ("vite-plugin-solid", "^2.11.12"),
    ("typescript", "^5.9.2"),
    ("@types/react", "^19.2.17"),
    ("@types/react-dom", "^19.2.3"),
    ("@types/node", "^26.1.1"),
    ("tailwindcss", "^4.3.2"),
    ("@sveltejs/kit", "^2.69.2"),
    ("@sveltejs/vite-plugin-svelte", "^7.2.0"),
    ("@sveltejs/adapter-auto", "^7.0.1"),
    ("svelte", "^5.56.4"),
    ("next", "^16.2.10"),
    ("nuxt", "^4.4.8"),
    ("@angular/core", "^22.0.6"),
    ("@angular/platform-browser", "^22.0.6"),
    ("@angular/platform-browser-dynamic", "^22.0.6"),
    ("@angular/router", "^22.0.6"),
    ("@angular/compiler", "^22.0.6"),
    ("@angular/common", "^22.0.6"),
    ("rxjs", "^7.8.2"),
    ("zone.js", "^0.16.2"),
    ("tslib", "^2.8.1"),
    ("@angular/cli", "^22.0.6"),
    ("@angular/compiler-cli", "^22.0.6"),
    ("@angular-devkit/build-angular", "^22.0.6"),
    ("@builder.io/qwik", "^1.20.0"),
    ("@builder.io/qwik-city", "^1.20.0"),
    ("astro", "^7.0.7"),
    ("express", "^5.2.1"),
    ("@types/express", "^5.0.6"),
    ("hono", "^4.12.30"),
    ("@hono/node-server", "^1.19.6"),
    ("@nestjs/core", "^11.1.28"),
    ("@nestjs/common", "^11.1.28"),
    ("@nestjs/platform-express", "^11.1.28"),
    ("reflect-metadata", "^0.2.2"),
    ("zod", "^4.4.3"),
    ("@trpc/server", "^11.18.0"),
    ("fastify", "^5.10.0"),
    ("tsx", "^4.23.0"),
];

fn install_hint_command() -> &'static str {
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
    {
        return "mg install";
    }

    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    {
        return "mg install-web";
    }

    #[allow(unreachable_code)]
    "mg install"
}

/// Find project root for web commands
fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = shared::find_project_root(&cwd)?.ok_or_else(|| {
        anyhow::anyhow!("No MegaGate project found (missing .megagate/project.toml or package.json in the current project)")
    })?;
    Ok(root)
}

/// Add web dependency
#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
) -> Result<()> {
    let root = project_root()?;
    let adapter = mg_web_adapter::WebAdapter::new();
    shared::add(
        &adapter, &root, packages, version, dev, exact, optional, peer, no_save, global,
    )
    .await
}

/// Remove web dependency
pub async fn remove(package: String) -> Result<()> {
    let root = project_root()?;
    let adapter = mg_web_adapter::WebAdapter::new();
    shared::remove(&adapter, &root, &package).await
}

/// List web packages
pub async fn list() -> Result<()> {
    let root = project_root()?;
    let adapter = mg_web_adapter::WebAdapter::new();
    shared::list(&adapter, &root).await
}

/// Update web packages
pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = mg_web_adapter::WebAdapter::new();
    shared::update(&adapter, &root, packages, install).await
}

/// Install web dependencies
pub async fn install(packages: Vec<String>, frozen: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = mg_web_adapter::WebAdapter::new();

    for pkg in &packages {
        let spinner = mg_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mg_types::PackageName::new(pkg)?;
        let opts = mg_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }

    for target in install_targets(&root)? {
        if target.join("package.json").exists() {
            shared::install_with_adapter(&adapter, &target, "mg add", frozen).await?;
        } else {
            native_install_target(&target)?;
        }
    }

    Ok(())
}

pub async fn dev_at_root(
    project_root: &Path,
    host: Option<String>,
    port: Option<u16>,
) -> Result<()> {
    let targets = dev_targets(project_root, host, port)?;
    if targets.len() == 1 {
        return run_single_dev_target(&targets[0]);
    }

    run_multi_dev_targets(&targets)
}

#[derive(Debug)]
struct DevLaunch {
    program: PathBuf,
    args: Vec<OsString>,
    envs: Vec<(OsString, OsString)>,
}

impl DevLaunch {
    fn describe(&self) -> String {
        let args = self
            .args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        if args.is_empty() {
            self.program.display().to_string()
        } else {
            format!("{} {}", self.program.display(), args)
        }
    }
}

#[derive(Debug, Deserialize)]
struct DevPackageJson {
    scripts: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceConfig {
    mode: Option<String>,
    layout: Option<WorkspaceLayout>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceLayout {
    apps_dir: Option<String>,
}

#[derive(Debug, Clone)]
struct DevTarget {
    dir: PathBuf,
    role: &'static str,
    script_name: &'static str,
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Clone)]
enum WebProjectMode {
    Standalone,
    FullstackSplit,
    Monorepo,
}

fn detect_dev_target(project_root: &Path) -> Result<PathBuf> {
    let root_script = read_dev_script(project_root)?;
    if root_script
        .as_deref()
        .is_some_and(|script| !script.starts_with("mg "))
    {
        return Ok(project_root.to_path_buf());
    }

    let frontend = workspace_frontend_dir(project_root)?;
    if frontend.join("package.json").exists() {
        return Ok(frontend);
    }

    if root_script.is_some() {
        return Ok(project_root.to_path_buf());
    }

    if infer_native_dev_launch(project_root, None, None).is_ok() {
        return Ok(project_root.to_path_buf());
    }

    bail!(
        "No runnable dev target found in '{}'. Run '{}' first.",
        project_root.display(),
        install_hint_command()
    )
}

fn workspace_frontend_dir(project_root: &Path) -> Result<PathBuf> {
    let workspace_path = project_root.join("megagate.workspace.toml");
    if workspace_path.exists() {
        let contents = std::fs::read_to_string(&workspace_path)?;
        let config: WorkspaceConfig = toml::from_str(&contents)?;
        if config.mode.as_deref() == Some("monorepo") {
            let apps_dir = config
                .layout
                .as_ref()
                .and_then(|layout| layout.apps_dir.as_deref())
                .unwrap_or("apps");
            return Ok(project_root.join(apps_dir).join("frontend"));
        }
    }
    Ok(project_root.join("apps").join("frontend"))
}

fn workspace_backend_dir(project_root: &Path) -> Result<PathBuf> {
    let workspace_path = project_root.join("megagate.workspace.toml");
    if workspace_path.exists() {
        let contents = std::fs::read_to_string(&workspace_path)?;
        let config: WorkspaceConfig = toml::from_str(&contents)?;
        if config.mode.as_deref() == Some("monorepo") {
            let apps_dir = config
                .layout
                .as_ref()
                .and_then(|layout| layout.apps_dir.as_deref())
                .unwrap_or("apps");
            return Ok(project_root.join(apps_dir).join("backend"));
        }
    }
    Ok(project_root.join("apps").join("backend"))
}

fn detect_project_mode(project_root: &Path) -> Result<WebProjectMode> {
    let workspace_path = project_root.join("megagate.workspace.toml");
    if workspace_path.exists() {
        let contents = std::fs::read_to_string(&workspace_path)?;
        let config: WorkspaceConfig = toml::from_str(&contents)?;
        if config.mode.as_deref() == Some("monorepo") {
            return Ok(WebProjectMode::Monorepo);
        }
    }

    if has_backend_manifest(&project_root.join("server")) {
        return Ok(WebProjectMode::FullstackSplit);
    }

    Ok(WebProjectMode::Standalone)
}

fn install_targets(project_root: &Path) -> Result<Vec<PathBuf>> {
    match detect_project_mode(project_root)? {
        WebProjectMode::Standalone => Ok(vec![project_root.to_path_buf()]),
        WebProjectMode::FullstackSplit => Ok(vec![
            project_root.to_path_buf(),
            project_root.join("server"),
        ]),
        WebProjectMode::Monorepo => {
            let mut targets = Vec::new();

            let frontend = workspace_frontend_dir(project_root)?;
            if frontend.join("package.json").exists() {
                targets.push(frontend);
            }

            let backend = workspace_backend_dir(project_root)?;
            if has_backend_manifest(&backend) {
                targets.push(backend);
            }

            let packages_dir = project_root.join("packages");
            if packages_dir.exists() {
                let mut package_paths = std::fs::read_dir(&packages_dir)?
                    .filter_map(|entry| entry.ok().map(|item| item.path()))
                    .collect::<Vec<_>>();
                package_paths.sort();
                for path in package_paths {
                    if path.join("package.json").exists() {
                        targets.push(path);
                    }
                }
            }

            Ok(targets)
        }
    }
}

fn dev_targets(
    project_root: &Path,
    host: Option<String>,
    port: Option<u16>,
) -> Result<Vec<DevTarget>> {
    let fullstack_backend_port = port.map(|value| value.saturating_add(1)).or(Some(3000));
    let monorepo_backend_port = port.map(|value| value.saturating_add(1)).or(Some(4000));

    match detect_project_mode(project_root)? {
        WebProjectMode::Standalone => Ok(vec![DevTarget {
            dir: detect_dev_target(project_root)?,
            role: "frontend",
            script_name: "dev",
            host,
            port,
        }]),
        WebProjectMode::FullstackSplit => Ok(vec![
            DevTarget {
                dir: project_root.to_path_buf(),
                role: "frontend",
                script_name: "dev:client",
                host,
                port,
            },
            DevTarget {
                dir: project_root.join("server"),
                role: "backend",
                script_name: "dev",
                host: None,
                port: fullstack_backend_port,
            },
        ]),
        WebProjectMode::Monorepo => Ok(vec![
            DevTarget {
                dir: workspace_frontend_dir(project_root)?,
                role: "frontend",
                script_name: "dev",
                host,
                port,
            },
            DevTarget {
                dir: workspace_backend_dir(project_root)?,
                role: "backend",
                script_name: "dev",
                host: None,
                port: monorepo_backend_port,
            },
        ]),
    }
}

fn read_script(project_root: &Path, script_name: &str) -> Result<Option<String>> {
    let package_json_path = project_root.join("package.json");
    if !package_json_path.exists() {
        return Ok(None);
    }

    let parsed: DevPackageJson =
        serde_json::from_str(&std::fs::read_to_string(package_json_path)?)?;
    let scripts = match parsed.scripts {
        Some(scripts) => scripts,
        None => return Ok(None),
    };

    if let Some(script) = scripts.get(script_name).cloned() {
        return Ok(Some(script));
    }

    if script_name == "dev" {
        return Ok(scripts.get("start").cloned());
    }

    Ok(None)
}

fn read_dev_script(project_root: &Path) -> Result<Option<String>> {
    read_script(project_root, "dev")
}

fn build_dev_launch(
    project_root: &Path,
    script_name: &str,
    host: Option<String>,
    port: Option<u16>,
) -> Result<DevLaunch> {
    let script = match read_script(project_root, script_name)? {
        Some(script) => script,
        None => return infer_native_dev_launch(project_root, host, port),
    };

    let tokens: Vec<&str> = script.split_whitespace().collect();
    if tokens.is_empty() {
        bail!(
            "Empty dev script in '{}'",
            project_root.join("package.json").display()
        );
    }

    match tokens.as_slice() {
        ["vite"] | ["vite", "dev"] => {
            let mut args = Vec::new();
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "vite")?,
                args,
                envs: vec![],
            })
        }
        ["vite", rest @ ..] => {
            let mut args: Vec<OsString> = rest.iter().map(OsString::from).collect();
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "vite")?,
                args,
                envs: vec![],
            })
        }
        ["next", "dev"] => {
            let mut args = vec![OsString::from("dev")];
            if let Some(host) = host {
                args.push(OsString::from("-H"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("-p"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "next")?,
                args,
                envs: vec![],
            })
        }
        ["next", "dev", rest @ ..] => {
            let mut args = vec![OsString::from("dev")];
            args.extend(rest.iter().map(OsString::from));
            if let Some(host) = host {
                args.push(OsString::from("-H"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("-p"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "next")?,
                args,
                envs: vec![],
            })
        }
        ["nuxt", "dev"] | ["nuxt", "dev", "--host"] => {
            let mut args = vec![OsString::from("dev")];
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "nuxt")?,
                args,
                envs: vec![
                    (
                        OsString::from("NUXT_TELEMETRY_DISABLED"),
                        OsString::from("1"),
                    ),
                    (
                        OsString::from("NUXT_TELEMETRY_CONSENT"),
                        OsString::from("0"),
                    ),
                ],
            })
        }
        ["nuxt", "dev", rest @ ..] => {
            let mut args = vec![OsString::from("dev")];
            args.extend(rest.iter().map(OsString::from));
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "nuxt")?,
                args,
                envs: vec![
                    (
                        OsString::from("NUXT_TELEMETRY_DISABLED"),
                        OsString::from("1"),
                    ),
                    (
                        OsString::from("NUXT_TELEMETRY_CONSENT"),
                        OsString::from("0"),
                    ),
                ],
            })
        }
        ["astro", "dev"] => {
            let mut args = vec![OsString::from("dev")];
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "astro")?,
                args,
                envs: vec![],
            })
        }
        ["astro", "dev", rest @ ..] => {
            let mut args = vec![OsString::from("dev")];
            args.extend(rest.iter().map(OsString::from));
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "astro")?,
                args,
                envs: vec![],
            })
        }
        ["remix", "vite:dev"] => {
            let mut args = vec![OsString::from("vite:dev")];
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "remix")?,
                args,
                envs: vec![],
            })
        }
        ["remix", "vite:dev", rest @ ..] => {
            let mut args = vec![OsString::from("vite:dev")];
            args.extend(rest.iter().map(OsString::from));
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "remix")?,
                args,
                envs: vec![],
            })
        }
        ["ng", "serve"] => {
            let mut args = vec![OsString::from("serve")];
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "ng")?,
                args,
                envs: vec![
                    (OsString::from("NG_CLI_ANALYTICS"), OsString::from("false")),
                    (OsString::from("CI"), OsString::from("1")),
                ],
            })
        }
        ["ng", "serve", rest @ ..] => {
            let mut args = vec![OsString::from("serve")];
            args.extend(rest.iter().map(OsString::from));
            if let Some(host) = host {
                args.push(OsString::from("--host"));
                args.push(OsString::from(host));
            }
            if let Some(port) = port {
                args.push(OsString::from("--port"));
                args.push(OsString::from(port.to_string()));
            }
            Ok(DevLaunch {
                program: resolve_local_bin(project_root, "ng")?,
                args,
                envs: vec![
                    (OsString::from("NG_CLI_ANALYTICS"), OsString::from("false")),
                    (OsString::from("CI"), OsString::from("1")),
                ],
            })
        }
        ["node", rest @ ..] => Ok(DevLaunch {
            program: PathBuf::from("node"),
            args: rest.iter().map(OsString::from).collect(),
            envs: vec![],
        }),
        ["tsx", rest @ ..] => Ok(DevLaunch {
            program: resolve_local_bin(project_root, "tsx")?,
            args: rest.iter().map(OsString::from).collect(),
            envs: vec![],
        }),
        _ => bail!(
            "Unsupported dev script '{}' in '{}'",
            script,
            project_root.join("package.json").display()
        ),
    }
}

fn has_backend_manifest(dir: &Path) -> bool {
    dir.join("package.json").exists()
        || dir.join("go.mod").exists()
        || dir.join("Cargo.toml").exists()
        || dir.join("manage.py").exists()
        || dir.join("main.py").exists()
        || dir.join("pom.xml").exists()
        || dir.join("artisan").exists()
        || dir.join("composer.json").exists()
}

fn infer_native_dev_launch(
    project_root: &Path,
    _host: Option<String>,
    port: Option<u16>,
) -> Result<DevLaunch> {
    if project_root.join("go.mod").exists() {
        return Ok(DevLaunch {
            program: PathBuf::from("go"),
            args: vec![OsString::from("run"), OsString::from(".")],
            envs: env_port_pairs(port),
        });
    }

    if project_root.join("manage.py").exists() {
        let bind = format!("0.0.0.0:{}", port.unwrap_or(3000));
        let python = native_python_program(project_root);
        return Ok(DevLaunch {
            program: python,
            args: vec![
                OsString::from("manage.py"),
                OsString::from("runserver"),
                OsString::from(bind),
            ],
            envs: env_port_pairs(port),
        });
    }

    if project_root.join("main.py").exists() {
        let python = native_python_program(project_root);
        return Ok(DevLaunch {
            program: python,
            args: vec![OsString::from("main.py")],
            envs: env_port_pairs(port),
        });
    }

    if project_root.join("Cargo.toml").exists() && project_root.join("src/main.rs").exists() {
        return Ok(DevLaunch {
            program: PathBuf::from("cargo"),
            args: vec![OsString::from("run")],
            envs: env_port_pairs(port),
        });
    }

    if project_root.join("artisan").exists() {
        return Ok(DevLaunch {
            program: PathBuf::from("php"),
            args: vec![
                OsString::from("artisan"),
                OsString::from("serve"),
                OsString::from("--host=0.0.0.0"),
                OsString::from(format!("--port={}", port.unwrap_or(8000))),
            ],
            envs: env_port_pairs(port),
        });
    }

    if project_root.join("composer.json").exists()
        && project_root.join("public/index.php").exists()
        && project_root.join("bin/console").exists()
    {
        return Ok(DevLaunch {
            program: PathBuf::from("php"),
            args: vec![
                OsString::from("-S"),
                OsString::from(format!("0.0.0.0:{}", port.unwrap_or(8000))),
                OsString::from("-t"),
                OsString::from("public"),
                OsString::from("public/index.php"),
            ],
            envs: env_port_pairs(port),
        });
    }

    if project_root.join("pom.xml").exists() {
        let pom = std::fs::read_to_string(project_root.join("pom.xml")).unwrap_or_default();
        if pom.contains("quarkus.platform.version") {
            let mut args = vec![
                OsString::from("quarkus:dev"),
                OsString::from("-Dquarkus.http.host=0.0.0.0"),
                OsString::from("-Dquarkus.analytics.disabled=true"),
            ];
            if let Some(port) = port {
                args.push(OsString::from(format!("-Dquarkus.http.port={port}")));
            }
            return Ok(DevLaunch {
                program: PathBuf::from("mvn"),
                args,
                envs: env_port_pairs(port),
            });
        }

        let mut args = vec![OsString::from("spring-boot:run")];
        if let Some(port) = port {
            args.push(OsString::from(format!(
                "-Dspring-boot.run.arguments=--server.port={port}"
            )));
        }
        return Ok(DevLaunch {
            program: PathBuf::from("mvn"),
            args,
            envs: env_port_pairs(port),
        });
    }

    bail!(
        "No '{}' script found in '{}' and no supported native dev entrypoint was detected",
        "dev",
        project_root.display()
    )
}

fn env_port_pairs(port: Option<u16>) -> Vec<(OsString, OsString)> {
    let mut envs = Vec::new();
    if let Some(port) = port {
        let port = port.to_string();
        envs.push((OsString::from("PORT"), OsString::from(port.clone())));
        envs.push((OsString::from("SERVER_PORT"), OsString::from(port.clone())));
        envs.push((OsString::from("QUARKUS_HTTP_PORT"), OsString::from(port)));
    }
    envs
}

fn native_python_program(project_root: &Path) -> PathBuf {
    let venv_python = native_venv_executable(project_root, "python");
    if venv_python.exists() {
        return venv_python;
    }

    #[cfg(windows)]
    {
        return PathBuf::from("python");
    }

    #[allow(unreachable_code)]
    PathBuf::from("python3")
}

fn native_pip_program(project_root: &Path) -> PathBuf {
    let venv_pip = native_venv_executable(project_root, "pip");
    if venv_pip.exists() {
        return venv_pip;
    }

    #[cfg(windows)]
    {
        return PathBuf::from("pip");
    }

    #[allow(unreachable_code)]
    PathBuf::from("pip3")
}

fn native_venv_executable(project_root: &Path, bin_name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        return project_root
            .join(".venv")
            .join("Scripts")
            .join(format!("{bin_name}.exe"));
    }

    #[allow(unreachable_code)]
    project_root.join(".venv").join("bin").join(bin_name)
}

fn native_install_target(project_root: &Path) -> Result<()> {
    if project_root.join("go.mod").exists() {
        info(&format!(
            "Installing native Go dependencies in {}",
            project_root.display()
        ));
        return run_native_install(project_root, "go", &["mod", "tidy"]);
    }

    if project_root.join("requirements.txt").exists() {
        info(&format!(
            "Installing native Python dependencies in {}",
            project_root.display()
        ));
        run_native_install(project_root, "python3", &["-m", "venv", ".venv"])?;
        return run_native_install(
            project_root,
            &native_pip_program(project_root).to_string_lossy(),
            &["install", "-r", "requirements.txt"],
        );
    }

    if project_root.join("Cargo.toml").exists() {
        info(&format!(
            "Fetching native Rust dependencies in {}",
            project_root.display()
        ));
        return run_native_install(project_root, "cargo", &["fetch"]);
    }

    if project_root.join("pom.xml").exists() {
        info(&format!(
            "Fetching native Maven dependencies in {}",
            project_root.display()
        ));
        return run_native_install(
            project_root,
            "mvn",
            &["-q", "-DskipTests", "dependency:go-offline"],
        );
    }

    if project_root.join("composer.json").exists() || project_root.join("artisan").exists() {
        info(&format!(
            "Installing native PHP dependencies in {}",
            project_root.display()
        ));
        return run_native_install(project_root, "composer", &["install"]);
    }

    bail!(
        "No supported native install flow found in '{}'",
        project_root.display()
    )
}

fn run_native_install(project_root: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(project_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to start native install '{}'", program))?;

    if status.success() {
        Ok(())
    } else {
        bail!(
            "native install command '{}' exited with status {}",
            format_args!("{} {}", program, args.join(" ")),
            status
        )
    }
}

fn run_single_dev_target(target: &DevTarget) -> Result<()> {
    let launch = build_dev_launch(
        &target.dir,
        target.script_name,
        target.host.clone(),
        target.port,
    )?;

    info(&format!(
        "Starting web dev server in {}",
        target.dir.display()
    ));
    info(&format!("  {}", launch.describe()));

    let local_bin = target.dir.join("node_modules").join(".bin");
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .current_dir(&target.dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("PATH", prepend_path(&local_bin)?);
    for (key, value) in &launch.envs {
        command.env(key, value);
    }
    if let Some(port) = target.port {
        command.env("PORT", port.to_string());
    }

    let status = command
        .status()
        .with_context(|| format!("failed to start '{}'", launch.program.to_string_lossy()))?;

    if status.success() {
        Ok(())
    } else {
        bail!("dev server exited with status {status}")
    }
}

fn run_multi_dev_targets(targets: &[DevTarget]) -> Result<()> {
    let mut children = Vec::new();

    for target in targets {
        let launch = build_dev_launch(
            &target.dir,
            target.script_name,
            target.host.clone(),
            target.port,
        )?;
        info(&format!(
            "Starting {} dev server in {}",
            target.role,
            target.dir.display()
        ));
        info(&format!("  {}", launch.describe()));

        let local_bin = target.dir.join("node_modules").join(".bin");
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(&target.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .env("PATH", prepend_path(&local_bin)?);
        for (key, value) in &launch.envs {
            command.env(key, value);
        }
        if let Some(port) = target.port {
            command.env("PORT", port.to_string());
        }

        let child = command
            .spawn()
            .with_context(|| format!("failed to start '{}'", launch.program.to_string_lossy()))?;
        children.push((target.role, child));
    }

    loop {
        for index in 0..children.len() {
            if let Some(status) = children[index].1.try_wait()? {
                for (other_index, (_, child)) in children.iter_mut().enumerate() {
                    if other_index != index {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
                if status.success() {
                    return Ok(());
                }
                bail!(
                    "{} dev server exited with status {status}",
                    children[index].0
                );
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn resolve_local_bin(project_root: &Path, bin_name: &str) -> Result<PathBuf> {
    let bin_dir = project_root.join("node_modules").join(".bin");
    let candidates = local_bin_candidates(&bin_dir, bin_name);
    if let Some(path) = candidates.into_iter().find(|path| path.exists()) {
        return Ok(path);
    }

    bail!(
        "Missing local executable '{}'. Run '{}' in '{}'.",
        bin_name,
        install_hint_command(),
        project_root.display()
    )
}

fn local_bin_candidates(bin_dir: &Path, bin_name: &str) -> Vec<PathBuf> {
    let candidates = vec![bin_dir.join(bin_name)];
    #[cfg(windows)]
    {
        let mut candidates = candidates;
        candidates.push(bin_dir.join(format!("{bin_name}.cmd")));
        candidates.push(bin_dir.join(format!("{bin_name}.exe")));
        return candidates;
    }
    candidates
}

fn prepend_path(local_bin: &Path) -> Result<OsString> {
    let mut paths = vec![local_bin.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    Ok(std::env::join_paths(paths)?)
}

// ── Scaffold (create) ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameworkRequest {
    raw: String,
    normalized: String,
    version: Option<String>,
}

pub async fn run_create(framework: &str, project_name: &str) -> Result<()> {
    run_create_with_options(framework, project_name, None).await
}

pub async fn run_create_with_options(
    framework: &str,
    project_name: &str,
    flags: Option<ScaffoldFlags>,
) -> Result<()> {
    let mut flags = flags.unwrap_or_default();

    if let Some(preset_name) = &flags.preset.clone() {
        apply_preset(preset_name, &mut flags);
    }

    let fe_framework = resolve_framework(Some(framework), &flags)?;
    validate_flags(&flags, &fe_framework)?;

    info(&format!(
        "Creating new web project '{}' with {}",
        project_name, fe_framework
    ));

    let config = build_web_config(&fe_framework, project_name, &flags)?;
    let project_dir = crate::scaffold::Scaffolder::scaffold(&config)?;

    let proj_config = mg_config::project::ProjectConfig::new(
        crate::scaffold::Scaffolder::display_name(&project_dir),
        "web",
    );
    proj_config.save(&project_dir)?;

    let frontend = parse_framework_request(&fe_framework);
    let be_name = detect_backend_framework(&flags);
    let backend = be_name
        .as_deref()
        .map(parse_framework_request)
        .or_else(|| fullstack_backend_framework(&frontend.normalized).map(parse_framework_request));
    enrich_web_project_manifest(&project_dir, &frontend, backend.as_ref(), &flags).await?;

    info(&format!("Project '{}' created!", project_dir.display()));
    info(&format!(
        "  cd {} && {}",
        project_name,
        install_hint_command()
    ));

    Ok(())
}

fn resolve_framework(pos: Option<&str>, flags: &ScaffoldFlags) -> Result<String> {
    if let Some(fw) = pos {
        return Ok(fw.to_string());
    }
    if flags.react { Ok("react".into()) }
    else if flags.next { Ok("next".into()) }
    else if flags.vue { Ok("vue".into()) }
    else if flags.nuxt { Ok("nuxt".into()) }
    else if flags.svelte { Ok("svelte".into()) }
    else if flags.sveltekit { Ok("sveltekit".into()) }
    else if flags.solid { Ok("solid".into()) }
    else if flags.astro { Ok("astro".into()) }
    else if flags.remix { Ok("remix".into()) }
    else {
        anyhow::bail!("No framework specified. Use a flag like --react, --next, --vue, or pass a framework name as the first argument.")
    }
}

fn detect_backend_framework(flags: &ScaffoldFlags) -> Option<String> {
    if flags.express { Some("express".into()) }
    else if flags.fastify { Some("fastify".into()) }
    else if flags.nestjs { Some("nestjs".into()) }
    else if flags.hono { Some("hono".into()) }
    else if flags.koa { Some("koa".into()) }
    else if flags.trpc { Some("trpc".into()) }
    else { None }
}

fn validate_flags(flags: &ScaffoldFlags, fe_framework: &str) -> Result<()> {
    let fe_count = [
        flags.react, flags.next, flags.vue, flags.nuxt,
        flags.svelte, flags.sveltekit, flags.solid, flags.astro, flags.remix,
    ].iter().filter(|&&b| b).count();

    if fe_count > 1 {
        anyhow::bail!("Multiple frontend frameworks specified. Choose only one (--react, --next, --vue, etc.).");
    }

    if flags.pinia && fe_framework != "vue" && fe_framework != "nuxt" {
        anyhow::bail!("--pinia requires --vue or --nuxt");
    }

    if flags.shadcn && !flags.tailwindcss {
        // auto-enable tailwindcss, don't error
    }

    Ok(())
}

fn apply_preset(name: &str, flags: &mut ScaffoldFlags) {
    match name {
        "t3" => {
            flags.next = true;
            flags.ts = true;
            flags.tailwindcss = true;
            flags.trpc = true;
            flags.prisma = true;
            flags.zod = true;
            flags.nextauth = true;
        }
        "mern" => {
            flags.react = true;
            flags.express = true;
            flags.mongoose = true;
            flags.mongodb = true;
            flags.zod = true;
            flags.jwt = true;
        }
        "jamstack" => {
            flags.astro = true;
            flags.tailwindcss = true;
            flags.dotenv = true;
            flags.vercel = true;
        }
        "saas" => {
            flags.next = true;
            flags.tailwindcss = true;
            flags.shadcn = true;
            flags.prisma = true;
            flags.postgres = true;
            flags.clerk = true;
            flags.docker = true;
        }
        "mevn" => {
            flags.vue = true;
            flags.express = true;
            flags.mongoose = true;
            flags.mongodb = true;
            flags.zod = true;
        }
        _ => {}
    }
}

fn build_web_config(
    framework: &str,
    project_name: &str,
    flags: &ScaffoldFlags,
) -> Result<crate::wizard::engine::ScaffoldConfig> {
    let frontend = parse_framework_request(framework);

    let mut config = if flags.monorepo {
        match detect_backend_framework(flags)
            .or_else(|| fullstack_backend_framework(&frontend.normalized).map(str::to_string))
        {
            Some(be) => {
                let backend = parse_framework_request(&be);
                crate::wizard::engine::ScaffoldConfig {
                    core: "web".to_string(),
                    sub_type: "monorepo".to_string(),
                    frameworks: vec![frontend.normalized.clone(), backend.normalized],
                    project_name: project_name.to_string(),
                    features: vec![],
                    template_dir: std::path::PathBuf::new(),
                }
            }
            None => {
                info("--monorepo ignored: no backend framework specified (add --express, --fastify, etc.)");
                crate::scaffold::Scaffolder::infer_web_create_config(
                    &frontend.normalized,
                    project_name,
                )?
            }
        }
    } else {
        crate::scaffold::Scaffolder::infer_web_create_config(&frontend.normalized, project_name)?
    };

    config.features = web_features(flags);

    if config.frameworks.len() >= 2 {
        let lang = &config.frameworks[0];
        if !config.features.contains(lang) {
            config.features.push(lang.clone());
        }
    }

    Ok(config)
}

fn web_features(flags: &ScaffoldFlags) -> Vec<String> {
    let mut features = Vec::new();

    // Language
    if flags.ts { features.push("typescript".into()); }
    if flags.js { features.push("javascript".into()); }

    // Styling
    if flags.tailwindcss || flags.shadcn || flags.daisyui { features.push("tailwindcss".into()); }
    if flags.css_modules { features.push("css-modules".into()); }
    if flags.styled_components { features.push("styled-components".into()); }
    if flags.sass { features.push("sass".into()); }
    if flags.unocss { features.push("unocss".into()); }
    if flags.shadcn { features.push("shadcn".into()); }
    if flags.daisyui { features.push("daisyui".into()); }

    // State
    if flags.zustand { features.push("zustand".into()); }
    if flags.redux { features.push("redux".into()); }
    if flags.jotai { features.push("jotai".into()); }
    if flags.recoil { features.push("recoil".into()); }
    if flags.pinia { features.push("pinia".into()); }
    if flags.tanstack_query { features.push("tanstack-query".into()); }

    // Backend
    if flags.express { features.push("express".into()); }
    if flags.fastify { features.push("fastify".into()); }
    if flags.nestjs { features.push("nestjs".into()); }
    if flags.hono { features.push("hono".into()); }
    if flags.koa { features.push("koa".into()); }
    if flags.trpc { features.push("trpc".into()); }

    // Database / ORM
    if flags.prisma { features.push("prisma".into()); }
    if flags.drizzle { features.push("drizzle".into()); }
    if flags.typeorm { features.push("typeorm".into()); }
    if flags.mongoose { features.push("mongoose".into()); }
    if flags.postgres { features.push("postgres".into()); }
    if flags.mysql { features.push("mysql".into()); }
    if flags.sqlite { features.push("sqlite".into()); }
    if flags.mongodb { features.push("mongodb".into()); }

    // Validation
    if flags.zod { features.push("zod".into()); }
    if flags.yup { features.push("yup".into()); }
    if flags.joi { features.push("joi".into()); }
    if flags.valibot { features.push("valibot".into()); }

    // Auth
    if flags.nextauth { features.push("nextauth".into()); }
    if flags.clerk { features.push("clerk".into()); }
    if flags.lucia { features.push("lucia".into()); }
    if flags.jwt { features.push("jwt".into()); }
    if flags.oauth { features.push("oauth".into()); }

    // Testing
    if flags.vitest { features.push("vitest".into()); }
    if flags.jest { features.push("jest".into()); }
    if flags.playwright { features.push("playwright".into()); }
    if flags.cypress { features.push("cypress".into()); }
    if flags.testing_library { features.push("testing-library".into()); }

    // Linting
    if flags.eslint { features.push("eslint".into()); }
    if flags.prettier { features.push("prettier".into()); }
    if flags.biome { features.push("biome".into()); }
    if flags.husky { features.push("husky".into()); }
    if flags.lint_staged { features.push("lint-staged".into()); }
    if flags.commitlint { features.push("commitlint".into()); }

    // Monorepo
    if flags.monorepo { features.push("monorepo".into()); }
    if flags.turborepo { features.push("turborepo".into()); }
    if flags.nx { features.push("nx".into()); }
    if flags.workspaces { features.push("workspaces".into()); }
    if flags.changesets { features.push("changesets".into()); }

    // API
    if flags.rest { features.push("rest".into()); }
    if flags.graphql { features.push("graphql".into()); }
    if flags.trpc_api { features.push("trpc-api".into()); }
    if flags.grpc { features.push("grpc".into()); }

    // Deployment
    if flags.docker { features.push("docker".into()); }
    if flags.github_actions { features.push("github-actions".into()); }
    if flags.vercel { features.push("vercel".into()); }
    if flags.railway { features.push("railway".into()); }
    if flags.fly { features.push("fly".into()); }

    // Misc
    if flags.dotenv { features.push("dotenv".into()); }
    if flags.i18n { features.push("i18n".into()); }
    if flags.pwa { features.push("pwa".into()); }
    if flags.storybook { features.push("storybook".into()); }
    if flags.sentry { features.push("sentry".into()); }
    if flags.analytics { features.push("analytics".into()); }

    // Extra
    for feature in &flags.features {
        if !features.contains(feature) {
            features.push(feature.clone());
        }
    }

    features
}

fn parse_framework_request(input: &str) -> FrameworkRequest {
    let (framework, version) = match input.rsplit_once('@') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            (name.to_string(), Some(version.to_string()))
        }
        _ => (input.to_string(), None),
    };

    FrameworkRequest {
        raw: input.to_string(),
        normalized: normalize_cli_web_framework(&framework),
        version,
    }
}

fn normalize_cli_web_framework(framework: &str) -> String {
    match framework {
        "react" | "react-app" => "react-vite".to_string(),
        "vue" | "vue-app" => "vue-vite".to_string(),
        "next" | "next-app" => "nextjs".to_string(),
        "svelte" => "sveltekit".to_string(),
        "solid" | "solid-app" => "solidjs".to_string(),
        "angular-app" => "angular".to_string(),
        "qwik-app" => "qwik".to_string(),
        other => other.to_string(),
    }
}

struct WebFrameworkSeed {
    name: &'static str,
    /// Packages that use the user's requested version
    primary: &'static [&'static str],
    /// Packages resolved independently from the registry latest path
    supplemental: &'static [&'static str],
    toolchain: &'static [WebToolchainPackage],
}

struct WebToolchainPackage {
    section: &'static str,
    package: &'static str,
    typescript_only: bool,
    /// If set, use this version instead of fetching from npm registry.
    version: Option<&'static str>,
}

const FRAMEWORK_SEEDS: &[WebFrameworkSeed] = &[
    WebFrameworkSeed {
        name: "vanilla",
        primary: &[],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "solidjs",
        primary: &["solid-js"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite-plugin-solid",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "sveltekit",
        primary: &["@sveltejs/kit"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@sveltejs/vite-plugin-svelte",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@sveltejs/adapter-auto",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "svelte",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: Some("^6.0.3"),
            },
        ],
    },
    WebFrameworkSeed {
        name: "react-vite",
        primary: &["react", "react-dom"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@vitejs/plugin-react",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react-dom",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "nextjs",
        primary: &["next"],
        supplemental: &["react", "react-dom"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/react-dom",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "fastify",
        primary: &["fastify"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "vue-vite",
        primary: &["vue"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@vitejs/plugin-vue",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "nuxt",
        primary: &["nuxt"],
        supplemental: &["vue"],
        toolchain: &[WebToolchainPackage {
            section: "devDependencies",
            package: "typescript",
            typescript_only: true,
            version: None,
        }],
    },
    WebFrameworkSeed {
        name: "angular",
        primary: &[
            "@angular/core",
            "@angular/compiler",
            "@angular/common",
            "@angular/platform-browser",
            "@angular/platform-browser-dynamic",
            "@angular/router",
        ],
        supplemental: &["rxjs", "zone.js", "tslib"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "@angular/cli",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@angular/compiler-cli",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@angular-devkit/build-angular",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "qwik",
        primary: &["@builder.io/qwik"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "@builder.io/qwik-city",
                typescript_only: false,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "vite",
                typescript_only: false,
                version: Some("^7.3.6"),
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "astro",
        primary: &["astro"],
        supplemental: &[],
        toolchain: &[WebToolchainPackage {
            section: "devDependencies",
            package: "typescript",
            typescript_only: true,
            version: None,
        }],
    },
    WebFrameworkSeed {
        name: "express",
        primary: &["express"],
        supplemental: &[],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/express",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "hono",
        primary: &["hono"],
        supplemental: &["@hono/node-server"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "nestjs",
        primary: &["@nestjs/core", "@nestjs/common", "@nestjs/platform-express"],
        supplemental: &["reflect-metadata", "rxjs"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
    WebFrameworkSeed {
        name: "trpc",
        primary: &["@trpc/server"],
        supplemental: &["express", "zod"],
        toolchain: &[
            WebToolchainPackage {
                section: "devDependencies",
                package: "typescript",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/express",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "@types/node",
                typescript_only: true,
                version: None,
            },
            WebToolchainPackage {
                section: "devDependencies",
                package: "tsx",
                typescript_only: true,
                version: None,
            },
        ],
    },
];

fn framework_primary_package(framework: &str) -> Option<String> {
    let seed_name = resolve_seed_name(framework);
    FRAMEWORK_SEEDS
        .iter()
        .find(|s| s.name == seed_name)
        .and_then(|s| s.primary.first())
        .copied()
        .map(str::to_string)
}

async fn fetch_npm_latest_version(package: &str) -> Result<String> {
    if let Some(version) = scaffold_version_override(package) {
        return Ok(version);
    }
    match fetch_npm_latest_version_from_registry(DEFAULT_NPM_REGISTRY, package).await {
        Ok(version) => Ok(version),
        Err(error) => {
            if let Some(version) = scaffold_baseline_version(package) {
                return Ok(version.to_string());
            }
            Err(error)
        }
    }
}

fn global_cli_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(50)
            .pool_idle_timeout(std::time::Duration::from_secs(120))
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent("MegaGate/0.1.0")
            .build()
            .expect("failed to build HTTP client")
    })
}

async fn fetch_npm_latest_version_from_registry(
    registry_url: &str,
    package: &str,
) -> Result<String> {
    let url = format!("{}/{package}/latest", registry_url.trim_end_matches('/'));
    let resp = global_cli_http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("network error fetching '{package}': {e}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("npm registry returned {} for '{package}'", resp.status());
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("bad npm response for '{package}': {e}"))?;
    parse_latest_version_response(package, &body)
}

fn parse_latest_version_response(package: &str, body: &serde_json::Value) -> Result<String> {
    body["version"]
        .as_str()
        .map(|s| format!("^{}", s))
        .ok_or_else(|| anyhow::anyhow!("no version field for '{package}'"))
}

fn scaffold_version_override(package: &str) -> Option<String> {
    std::env::var(SCAFFOLD_VERSION_OVERRIDES_ENV)
        .ok()
        .and_then(|raw| {
            raw.split(',')
                .filter_map(|entry| entry.trim().split_once('='))
                .find_map(|(name, version)| {
                    (name.trim() == package && !version.trim().is_empty())
                        .then(|| version.trim().to_string())
                })
        })
}

fn fullstack_backend_framework(framework: &str) -> Option<&'static str> {
    match framework {
        "react-fastify" => Some("fastify"),
        "react-spring" => Some("spring-boot"),
        "vue-laravel" => Some("laravel"),
        "react-express" => Some("express"),
        "react-hono" => Some("hono"),
        "react-nestjs" => Some("nestjs"),
        "react-trpc" => Some("trpc"),
        "vue-express" => Some("express"),
        "vue-hono" => Some("hono"),
        "vue-nestjs" => Some("nestjs"),
        "svelte-express" => Some("express"),
        "svelte-hono" => Some("hono"),
        "next" | "nextjs" => Some("express"),
        "nuxt" | "nuxtjs" => Some("hono"),
        _ => None,
    }
}

fn resolve_seed_name(framework: &str) -> &str {
    match framework {
        "react-fastify" | "react-spring" | "react-express" | "react-hono" | "react-nestjs"
        | "react-trpc" => "react-vite",
        "vue-laravel" | "vue-express" | "vue-hono" | "vue-nestjs" => "vue-vite",
        "svelte-express" | "svelte-hono" => "sveltekit",
        _ => framework,
    }
}

fn scaffold_baseline_version(package: &str) -> Option<&'static str> {
    SCAFFOLD_BASELINE_VERSIONS
        .iter()
        .find_map(|(name, version)| (*name == package).then_some(*version))
}

async fn resolve_primary_version(request: &FrameworkRequest) -> Result<String> {
    match request.version.as_deref() {
        Some("latest") | None => {
            let pkg = framework_primary_package(&request.normalized).ok_or_else(|| {
                anyhow::anyhow!(
                    "framework '{}' does not declare a primary package",
                    request.normalized
                )
            })?;
            fetch_npm_latest_version(&pkg).await
        }
        Some(version) => Ok(version.to_string()),
    }
}

fn ensure_package(root: &mut Map<String, Value>, section: &str, package: &str, version: &str) {
    let entry = root
        .entry(section.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(map) = entry {
        map.insert(package.to_string(), Value::String(version.to_string()));
    }
}

async fn apply_web_manifest_seed(
    package_json_path: &Path,
    request: &FrameworkRequest,
    flags: &ScaffoldFlags,
) -> Result<()> {
    if !package_json_path.exists() {
        return Ok(());
    }
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(package_json_path)?)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("package.json root must be an object"))?;

    let seed_name = resolve_seed_name(&request.normalized);
    if let Some(seed) = FRAMEWORK_SEEDS.iter().find(|s| s.name == seed_name) {
        if !seed.primary.is_empty() {
            let primary = resolve_primary_version(request).await?;
            for &package in seed.primary {
                ensure_package(object, "dependencies", package, &primary);
            }
        }
        for &package in seed.supplemental {
            let version = scaffold_baseline_version(package)
                .map(str::to_string)
                .unwrap_or(fetch_npm_latest_version(package).await?);
            ensure_package(object, "dependencies", package, &version);
        }
        for tool in seed.toolchain {
            if tool.typescript_only && !flags.ts {
                continue;
            }
            let version = match tool.version {
                Some(v) => v.to_string(),
                None => scaffold_baseline_version(tool.package)
                    .map(str::to_string)
                    .unwrap_or(fetch_npm_latest_version(tool.package).await?),
            };
            ensure_package(object, tool.section, tool.package, &version);
        }
    }

    if flags.tailwindcss || flags.shadcn || flags.daisyui {
        let version = fetch_npm_latest_version("tailwindcss").await?;
        ensure_package(object, "devDependencies", "tailwindcss", &version);
    }

    std::fs::write(package_json_path, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

async fn enrich_web_project_manifest(
    project_dir: &Path,
    frontend: &FrameworkRequest,
    backend: Option<&FrameworkRequest>,
    flags: &ScaffoldFlags,
) -> Result<()> {
    if flags.monorepo {
        apply_web_manifest_seed(
            &project_dir
                .join("apps")
                .join("frontend")
                .join("package.json"),
            frontend,
            flags,
        )
        .await?;
        if let Some(backend) = backend {
            apply_web_manifest_seed(
                &project_dir
                    .join("apps")
                    .join("backend")
                    .join("package.json"),
                backend,
                flags,
            )
            .await?;
        }
        return Ok(());
    }
    if let Some(backend) = backend {
        apply_web_manifest_seed(&project_dir.join("package.json"), frontend, flags).await?;
        apply_web_manifest_seed(
            &project_dir.join("server").join("package.json"),
            backend,
            flags,
        )
        .await
    } else {
        apply_web_manifest_seed(&project_dir.join("package.json"), frontend, flags).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn scaffold_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_parse_framework_request_supports_alias_and_version() {
        let request = parse_framework_request("react@latest");
        assert_eq!(request.normalized, "react-vite");
        assert_eq!(request.version.as_deref(), Some("latest"));
    }

    #[test]
    fn test_create_web_with_flags_seeds_package_json() {
        let _guard = scaffold_env_lock().lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("cli-react");
        let flags = ScaffoldFlags {
            ts: true,
            tailwindcss: true,
            ..Default::default()
        };
        std::env::set_var(
            SCAFFOLD_VERSION_OVERRIDES_ENV,
             "vite=^8.1.4,@vitejs/plugin-react=^6.0.3,typescript=^5.9.2,@types/react=^19.2.17,@types/react-dom=^19.2.3,tailwindcss=^4.3.2",
        );

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime
            .block_on(run_create_with_options(
                "react@18.3.1",
                &project.to_string_lossy(),
                Some(flags),
            ))
            .map(|_| ());
        std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);
        result.unwrap();

        let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
        assert!(package_json.contains("\"react\": \"18.3.1\""));
        assert!(package_json.contains("\"react-dom\": \"18.3.1\""));
        assert!(package_json.contains("\"vite\": \"^8.1.4\""));
        assert!(package_json.contains("\"@vitejs/plugin-react\": \"^6.0.3\""));
        assert!(package_json.contains("\"tailwindcss\": \"^4.3.2\""));
    }

    #[test]
    fn test_parse_latest_version_response_errors_without_version_field() {
        let err = parse_latest_version_response("vite", &serde_json::json!({ "name": "vite" }))
            .unwrap_err();
        assert!(err.to_string().contains("no version field"));
    }

    #[test]
    fn test_scaffold_version_override_short_circuits_network_resolution() {
        let _guard = scaffold_env_lock().lock().unwrap();
        std::env::set_var(
            SCAFFOLD_VERSION_OVERRIDES_ENV,
            "vite=^8.1.4,tailwindcss=^4.3.2",
        );
        let vite = scaffold_version_override("vite");
        let react = scaffold_version_override("react");
        std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

        assert_eq!(vite.as_deref(), Some("^8.1.4"));
        assert_eq!(react, None);
    }

    #[test]
    fn test_scaffold_baseline_version_covers_core_web_seed_packages() {
        assert_eq!(scaffold_baseline_version("react"), Some("^19.2.7"));
        assert_eq!(scaffold_baseline_version("vue"), Some("^3.5.39"));
        assert_eq!(scaffold_baseline_version("vite"), Some("^8.1.4"));
        assert_eq!(scaffold_baseline_version("next"), Some("^16.2.10"));
        assert_eq!(scaffold_baseline_version("typescript"), Some("^5.9.2"));
        assert_eq!(
            scaffold_baseline_version("@angular-devkit/build-angular"),
            Some("^22.0.6")
        );
        assert_eq!(scaffold_baseline_version("unknown-package"), None);
    }

    #[test]
    fn test_create_qwik_uses_framework_specific_vite_pin() {
        let _guard = scaffold_env_lock().lock().unwrap();
        std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("offline-qwik");
        let flags = ScaffoldFlags { ts: true, ..Default::default() };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_create_with_options(
                "qwik",
                &project.to_string_lossy(),
                Some(flags),
            ))
            .unwrap();

        let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
        assert!(package_json.contains("\"@builder.io/qwik\": \"^1.20.0\""));
        assert!(package_json.contains("\"@builder.io/qwik-city\": \"^1.20.0\""));
        assert!(package_json.contains("\"vite\": \"^7.3.6\""));
        assert!(package_json.contains("\"dev\": \"vite --mode ssr\""));
        assert!(!package_json.contains("\"vite\": \"^8.1.4\""));
    }

    #[test]
    fn test_create_web_without_overrides_uses_curated_baseline_when_registry_is_unavailable() {
        let _guard = scaffold_env_lock().lock().unwrap();
        std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("offline-react");
        let flags = ScaffoldFlags { ts: true, tailwindcss: true, ..Default::default() };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_create_with_options(
                "react@18.3.1",
                &project.to_string_lossy(),
                Some(flags),
            ))
            .unwrap();

        let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
        assert!(package_json.contains("\"react\": \"18.3.1\""));
        assert!(package_json.contains("\"react-dom\": \"18.3.1\""));
        assert!(package_json.contains("\"vite\": \"^8.1.4\""));
        assert!(package_json.contains("\"@vitejs/plugin-react\": \"^6.0.3\""));
        assert!(package_json.contains("\"typescript\": \"^5.9.2\""));
        assert!(package_json.contains("\"tailwindcss\": \"^4.3.2\""));
    }

    #[test]
    fn test_create_vanilla_web_without_primary_dependency_uses_toolchain_seed_only() {
        let _guard = scaffold_env_lock().lock().unwrap();
        std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("offline-vanilla");
        let flags = ScaffoldFlags { ts: true, ..Default::default() };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_create_with_options(
                "vanilla",
                &project.to_string_lossy(),
                Some(flags),
            ))
            .unwrap();

        let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
        assert!(package_json.contains("\"vite\": \"^8.1.4\""));
        assert!(package_json.contains("\"typescript\": \"^5.9.2\""));
        assert!(!package_json.contains("\"vanilla\""));
    }

    #[test]
    fn test_create_nextjs_uses_baseline_typescript_instead_of_registry_latest() {
        let _guard = scaffold_env_lock().lock().unwrap();
        std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("offline-next");
        let flags = ScaffoldFlags { ts: true, ..Default::default() };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_create_with_options(
                "nextjs",
                &project.to_string_lossy(),
                Some(flags),
            ))
            .unwrap();

        let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
        assert!(package_json.contains("\"next\": \"^16.2.10\""));
        assert!(package_json.contains("\"typescript\": \"^5.9.2\""));
        assert!(package_json.contains("\"@types/node\": \"^26.1.1\""));
    }

    #[test]
    fn test_build_dev_launch_for_vite_adds_host_and_port() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "scripts": { "dev": "vite" }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(dir.path().join("node_modules/.bin/vite"), "").unwrap();

        let launch =
            build_dev_launch(dir.path(), "dev", Some("127.0.0.1".into()), Some(4315)).unwrap();

        assert!(launch.program.ends_with("vite"));
        assert_eq!(
            launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["--host", "127.0.0.1", "--port", "4315"]
        );
    }

    #[test]
    fn test_build_dev_launch_falls_back_to_start_for_angular() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "scripts": { "start": "ng serve" }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(dir.path().join("node_modules/.bin/ng"), "").unwrap();

        let launch =
            build_dev_launch(dir.path(), "dev", Some("127.0.0.1".into()), Some(4315)).unwrap();

        assert!(launch.program.ends_with("ng"));
        assert_eq!(
            launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["serve", "--host", "127.0.0.1", "--port", "4315"]
        );
    }

    #[test]
    fn test_build_dev_launch_supports_nuxt_and_astro() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
        std::fs::write(dir.path().join("node_modules/.bin/nuxt"), "").unwrap();
        std::fs::write(dir.path().join("node_modules/.bin/astro"), "").unwrap();

        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "scripts": { "dev": "nuxt dev" }
            })
            .to_string(),
        )
        .unwrap();
        let nuxt_launch =
            build_dev_launch(dir.path(), "dev", Some("127.0.0.1".into()), Some(4315)).unwrap();
        assert!(nuxt_launch.program.ends_with("nuxt"));
        assert_eq!(
            nuxt_launch
                .envs
                .iter()
                .map(|(key, value)| (
                    key.to_string_lossy().to_string(),
                    value.to_string_lossy().to_string()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("NUXT_TELEMETRY_DISABLED".to_string(), "1".to_string()),
                ("NUXT_TELEMETRY_CONSENT".to_string(), "0".to_string()),
            ]
        );

        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "scripts": { "dev": "astro dev" }
            })
            .to_string(),
        )
        .unwrap();
        let astro_launch =
            build_dev_launch(dir.path(), "dev", Some("127.0.0.1".into()), Some(4315)).unwrap();
        assert!(astro_launch.program.ends_with("astro"));
        assert!(astro_launch.envs.is_empty());

        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "scripts": { "dev": "remix vite:dev" }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(dir.path().join("node_modules/.bin/remix"), "").unwrap();
        let remix_launch =
            build_dev_launch(dir.path(), "dev", Some("127.0.0.1".into()), Some(4315)).unwrap();
        assert!(remix_launch.program.ends_with("remix"));
        assert_eq!(
            remix_launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec![
                "vite:dev".to_string(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "4315".to_string(),
            ]
        );
    }

    #[test]
    fn test_build_dev_launch_supports_native_go_python_and_rust_backends() {
        let go_dir = tempfile::tempdir().unwrap();
        std::fs::write(go_dir.path().join("go.mod"), "module demo\n").unwrap();
        let go_launch = build_dev_launch(go_dir.path(), "dev", None, Some(4401)).unwrap();
        assert_eq!(go_launch.program, PathBuf::from("go"));
        assert_eq!(
            go_launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["run".to_string(), ".".to_string()]
        );

        let py_dir = tempfile::tempdir().unwrap();
        std::fs::write(py_dir.path().join("main.py"), "print('ok')\n").unwrap();
        let py_launch = build_dev_launch(py_dir.path(), "dev", None, Some(4402)).unwrap();
        assert_eq!(py_launch.program, PathBuf::from("python3"));
        assert_eq!(
            py_launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["main.py".to_string()]
        );

        let rust_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(rust_dir.path().join("src")).unwrap();
        std::fs::write(
            rust_dir.path().join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(rust_dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        let rust_launch = build_dev_launch(rust_dir.path(), "dev", None, Some(4403)).unwrap();
        assert_eq!(rust_launch.program, PathBuf::from("cargo"));
        assert_eq!(
            rust_launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["run".to_string()]
        );
    }

    #[test]
    fn test_build_dev_launch_supports_native_symfony_and_quarkus_backends() {
        let symfony_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(symfony_dir.path().join("public")).unwrap();
        std::fs::create_dir_all(symfony_dir.path().join("bin")).unwrap();
        std::fs::write(symfony_dir.path().join("composer.json"), "{}").unwrap();
        std::fs::write(symfony_dir.path().join("public/index.php"), "<?php\n").unwrap();
        std::fs::write(
            symfony_dir.path().join("bin/console"),
            "#!/usr/bin/env php\n",
        )
        .unwrap();

        let symfony_launch = build_dev_launch(symfony_dir.path(), "dev", None, Some(4404)).unwrap();
        assert_eq!(symfony_launch.program, PathBuf::from("php"));
        assert_eq!(
            symfony_launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec![
                "-S".to_string(),
                "0.0.0.0:4404".to_string(),
                "-t".to_string(),
                "public".to_string(),
                "public/index.php".to_string(),
            ]
        );

        let quarkus_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            quarkus_dir.path().join("pom.xml"),
            r#"<project><properties><quarkus.platform.version>3.6.0</quarkus.platform.version></properties></project>"#,
        )
        .unwrap();
        let quarkus_launch = build_dev_launch(quarkus_dir.path(), "dev", None, Some(4405)).unwrap();
        assert_eq!(quarkus_launch.program, PathBuf::from("mvn"));
        assert_eq!(
            quarkus_launch
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec![
                "quarkus:dev".to_string(),
                "-Dquarkus.http.host=0.0.0.0".to_string(),
                "-Dquarkus.analytics.disabled=true".to_string(),
                "-Dquarkus.http.port=4405".to_string(),
            ]
        );
    }

    #[test]
    fn test_detect_project_mode_supports_non_node_split_backends() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("server")).unwrap();
        std::fs::write(dir.path().join("server/go.mod"), "module demo\n").unwrap();

        let mode = detect_project_mode(dir.path()).unwrap();
        assert!(matches!(mode, WebProjectMode::FullstackSplit));
    }

    #[test]
    fn test_detect_dev_target_prefers_monorepo_frontend_when_root_script_is_mg() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("apps/frontend")).unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "root",
                "version": "0.1.0",
                "scripts": { "dev": "mg --core web dev" }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("megagate.workspace.toml"),
            r#"
mode = "monorepo"

[layout]
apps_dir = "apps"
"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("apps/frontend/package.json"),
            serde_json::json!({
                "name": "frontend",
                "version": "0.1.0",
                "scripts": { "dev": "vite" }
            })
            .to_string(),
        )
        .unwrap();

        let target = detect_dev_target(dir.path()).unwrap();
        assert_eq!(target, dir.path().join("apps/frontend"));
    }

    #[test]
    fn test_install_targets_cover_fullstack_and_monorepo_children() {
        let fullstack = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fullstack.path().join("server")).unwrap();
        std::fs::write(fullstack.path().join("package.json"), "{}").unwrap();
        std::fs::write(fullstack.path().join("server/package.json"), "{}").unwrap();

        let fullstack_targets = install_targets(fullstack.path()).unwrap();
        assert_eq!(
            fullstack_targets,
            vec![
                fullstack.path().to_path_buf(),
                fullstack.path().join("server")
            ]
        );

        let monorepo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(monorepo.path().join("apps/frontend")).unwrap();
        std::fs::create_dir_all(monorepo.path().join("apps/backend")).unwrap();
        std::fs::create_dir_all(monorepo.path().join("packages/contracts")).unwrap();
        std::fs::write(monorepo.path().join("apps/frontend/package.json"), "{}").unwrap();
        std::fs::write(monorepo.path().join("apps/backend/package.json"), "{}").unwrap();
        std::fs::write(
            monorepo.path().join("packages/contracts/package.json"),
            "{}",
        )
        .unwrap();
        std::fs::write(
            monorepo.path().join("megagate.workspace.toml"),
            r#"
mode = "monorepo"

[layout]
apps_dir = "apps"
"#,
        )
        .unwrap();

        let monorepo_targets = install_targets(monorepo.path()).unwrap();
        assert_eq!(
            monorepo_targets,
            vec![
                monorepo.path().join("apps/frontend"),
                monorepo.path().join("apps/backend"),
                monorepo.path().join("packages/contracts")
            ]
        );
    }

    #[test]
    fn test_install_targets_cover_monorepo_native_backend_children() {
        let monorepo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(monorepo.path().join("apps/frontend")).unwrap();
        std::fs::create_dir_all(monorepo.path().join("apps/backend")).unwrap();
        std::fs::write(monorepo.path().join("apps/frontend/package.json"), "{}").unwrap();
        std::fs::write(
            monorepo.path().join("apps/backend/main.py"),
            "print('backend')\n",
        )
        .unwrap();
        std::fs::write(
            monorepo.path().join("apps/backend/requirements.txt"),
            "fastapi>=0.115.0\n",
        )
        .unwrap();
        std::fs::write(
            monorepo.path().join("megagate.workspace.toml"),
            r#"
mode = "monorepo"

[layout]
apps_dir = "apps"
"#,
        )
        .unwrap();

        let monorepo_targets = install_targets(monorepo.path()).unwrap();
        assert_eq!(
            monorepo_targets,
            vec![
                monorepo.path().join("apps/frontend"),
                monorepo.path().join("apps/backend")
            ]
        );
    }

    #[test]
    fn test_native_python_program_prefers_local_venv_layout() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let venv_python = dir.path().join(".venv").join("Scripts").join("python.exe");
        #[cfg(not(windows))]
        let venv_python = dir.path().join(".venv").join("bin").join("python");
        std::fs::create_dir_all(venv_python.parent().unwrap()).unwrap();
        std::fs::write(&venv_python, "").unwrap();

        assert_eq!(native_python_program(dir.path()), venv_python);
    }

    #[test]
    fn test_native_pip_program_prefers_local_venv_layout() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let venv_pip = dir.path().join(".venv").join("Scripts").join("pip.exe");
        #[cfg(not(windows))]
        let venv_pip = dir.path().join(".venv").join("bin").join("pip");
        std::fs::create_dir_all(venv_pip.parent().unwrap()).unwrap();
        std::fs::write(&venv_pip, "").unwrap();

        assert_eq!(native_pip_program(dir.path()), venv_pip);
    }

    #[test]
    fn test_create_web_writes_project_toml_for_monorepo() {
        let _guard = scaffold_env_lock().lock().unwrap();
        std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("monorepo-fastapi");
        let flags = ScaffoldFlags {
            ts: true,
            monorepo: true,
            express: true,
            ..Default::default()
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_create_with_options(
                "react",
                &project.to_string_lossy(),
                Some(flags),
            ))
            .unwrap();

        let project_toml = project.join(".megagate").join("project.toml");
        assert!(project_toml.exists());
        let contents = std::fs::read_to_string(project_toml).unwrap();
        assert!(contents.contains("ecosystem = \"web\""));
    }

    #[test]
    fn test_dev_targets_for_fullstack_include_frontend_and_backend() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("server")).unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "root",
                "version": "0.1.0",
                "scripts": { "dev": "mg --core web dev" }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("server/package.json"),
            serde_json::json!({
                "name": "backend",
                "version": "0.1.0",
                "scripts": { "dev": "tsx watch src/server.ts" }
            })
            .to_string(),
        )
        .unwrap();

        let targets = dev_targets(dir.path(), Some("127.0.0.1".into()), Some(4318)).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].dir, dir.path());
        assert_eq!(targets[0].port, Some(4318));
        assert_eq!(targets[1].dir, dir.path().join("server"));
        assert_eq!(targets[1].port, Some(4319));
    }
}
