//! Centralized CLI error messages (English only — RULE §7).
//! Mọi error message của CLI định nghĩa tập trung tại đây.

use anyhow::{anyhow, Error};

/// `mg remove-ai <pkg> [pkg...]` — name packages to remove
pub fn remove_ai_usage() -> Error {
    anyhow!("mg remove-ai <pkg> [pkg...] — name the packages to remove")
}

/// `mg remove-app <pkg> [pkg...]` — name packages to remove
pub fn remove_app_usage() -> Error {
    anyhow!("mg remove-app <pkg> [pkg...] — name the packages to remove")
}

/// `mg add-app <pkg> [pkg...]` — name packages to add
pub fn add_app_usage() -> Error {
    anyhow!("mg add-app <pkg> [pkg...] — name the packages to add")
}

/// verb not applicable to cicd core (07 §4)
pub fn cicd_verb_not_applicable(verb: &str) -> Error {
    anyhow!(
        "'{verb}' does not apply to the cicd core (07 §4) — use `mg ci generate`, `mg verify`, or `mg deploy`."
    )
}

/// templates/web missing on disk
pub fn web_templates_missing(root: &std::path::Path) -> Error {
    anyhow!("templates/web does not exist on disk: {}", root.display())
}

/// no web layer found with template.toml+sources
pub fn no_web_layer_found(dir: &std::path::Path) -> Error {
    anyhow!(
        "No web layer with template.toml+sources found in {}",
        dir.display()
    )
}

/// template publish required before fetch
pub fn template_not_published(url: &str, http: u16) -> Error {
    anyhow!("Template '{}' not found at {} (HTTP {}) — run `mg template publish` first", url, url, http)
}

/// HF request failed
pub fn hf_request_failed() -> Error {
    anyhow!("HF request failed")
}

/// pull manifest failed — registry running?
pub fn pull_manifest_failed() -> Error {
    anyhow!("pull manifest failed (is the registry running? `mg registry serve`)")
}

/// parse manifest failed
pub fn parse_manifest_failed() -> Error {
    anyhow!("parse manifest failed")
}

/// file does not exist
pub fn file_not_found(path: &std::path::Path) -> Error {
    anyhow!("file does not exist: {}", path.display())
}

/// path does not exist
pub fn path_not_found(p: &std::path::Path) -> Error {
    anyhow!("path does not exist: {}", p.display())
}

/// directory has no files
pub fn dir_has_no_files(p: &std::path::Path) -> Error {
    anyhow!("directory has no files: {}", p.display())
}

/// push model failed — server running?
pub fn push_model_failed() -> Error {
    anyhow!("push model failed — is the server running? (`mg registry serve`)")
}

/// catalog listing failed
pub fn catalog_failed() -> Error {
    anyhow!("catalog listing failed")
}

/// token not found (revoked?)
pub fn token_not_found(token: &str) -> Error {
    anyhow!("Token {} not found (was it revoked?)", token)
}

/// web project missing package.json — cannot run tests
pub fn web_missing_package_json() -> Error {
    anyhow!("web project is missing package.json — cannot run tests")
}

/// package.json missing scripts.test
pub fn package_json_missing_test_script() -> Error {
    anyhow!("package.json is missing scripts.test")
}

/// lib core has no test runner for this language yet
pub fn lib_no_test_runner() -> Error {
    anyhow!("lib core has no test runner for this language yet (07 §4 P1)")
}

/// deploy target missing `stack` in [deploy] targets
pub fn deploy_target_missing_stack() -> Error {
    anyhow!(
        "aws target is missing `stack` in [deploy] targets — example: stack = \"my-infra\""
    )
}

/// deploy target adapter unknown
pub fn deploy_target_unknown(other: &str) -> Error {
    anyhow!(
        "deploy target '{other}' has no adapter — supported: aws | cloudflare | gcp"
    )
}

/// no deploy targets configured
pub fn no_deploy_targets() -> Error {
    anyhow!("no deploy target — add [deploy] targets to mg.toml")
}

/// python -m build failed
pub fn python_build_failed(e: &dyn std::fmt::Display) -> Error {
    anyhow!("python -m build failed: {e} — install `pip install build` in this project first")
}

/// flash only supports esp32-rust currently
pub fn flash_framework_unsupported(framework: &str) -> Error {
    anyhow!(
        "'mg flash' currently supports only the esp32-rust framework (you are using {framework}) — platformio/zephyr flash is P2"
    )
}

/// cloudflare workers build/deploy belongs to cicd core
pub fn cloudflare_build_in_cicd_core() -> Error {
    anyhow!(
        "cloudflare workers build/deploy belongs to the cicd core (Q12) — cloud core does not handle it as P1"
    )
}

/// unknown hardware package for add-hardware
pub fn unknown_hardware_package(pkg: &str) -> Error {
    anyhow!("unknown hardware package '{pkg}' (optimizer | bench)")
}

// ===== chung (nhiều file dùng) =====

/// cwd đã bị xóa khi resolve
pub fn cwd_deleted(e: &std::io::Error) -> Error {
    anyhow!("failed to resolve current working directory — has it been deleted?: {e}")
}

/// không detect được project theo core kind
pub fn no_mg_project_found(kind: &str) -> Error {
    let msg = match kind {
        "game" => "No MegaGate game project found (missing mg.toml with ecosystem = \"game\" \
                   or project.godot/Packages/manifest.json/.uproject/Cargo.toml in the current project)",
        "iot" => "No MegaGate IoT project found (missing mg.toml with ecosystem = \"iot\" \
                  or platformio.ini/west.yml/Cargo.toml in the current project)",
        "lib" | "library" => "No MegaGate library project found (missing mg.toml with \
                  ecosystem = \"lib\" or Cargo.toml/package.json/pyproject.toml in the current project)",
        "clo" | "cloud" => "No MegaGate cloud project found (missing mg.toml with ecosystem = \"cloud\" \
                  or Pulumi.yaml/*.tf/cdk package.json in the current project)",
        "app" => "No MegaGate app project found (missing mg.toml with ecosystem = \"app\" \
                  or pubspec.yaml/build.gradle/.kts/Package.swift in the current project)",
        "web" => "No MegaGate project found (missing .megagate/project.toml or package.json in the current project)",
        _ => "No MegaGate project found (missing mg.toml or known project manifest in the current directory tree)",
    };
    anyhow!(msg)
}

/// router core không biết core
pub fn unknown_core(core: &str) -> Error {
    anyhow!("Unknown core '{core}'")
}

/// chưa chạy mg init
pub fn project_root_missing() -> Error {
    anyhow!("Project root not found — run mg init")
}

/// thiếu mg.toml
pub fn mg_toml_missing() -> Error {
    anyhow!("mg.toml not found — run mg init")
}

/// không detect framework theo loại
pub fn no_framework_detected(framework: &str, root: &std::path::Path) -> Error {
    anyhow!("No {framework} framework detected in {}", root.display())
}

/// tool chạy fail (generic)
pub fn tool_failed(tool: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!("{tool} failed: {e}")
}

/// vượt MAX_PACKAGES
pub fn too_many_packages(count: usize, verb: &str) -> Error {
    anyhow!("Too many packages ({count}). Maximum per {verb} command is 50.")
}

/// lockfile mới hơn mg hỗ trợ
pub fn lockfile_newer(version: u32, supported: u32) -> Error {
    anyhow!(
        "mg.lock version {version} is newer than this version of mg (supports up to {supported}). \
         Upgrade mg to read this lockfile."
    )
}

/// dependency id trong lockfile không parse được
pub fn invalid_dep_id(dep: &str, name: &str, version: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!(
        "invalid dependency id '{dep}' in lockfile package '{name}@{version}': {e}"
    )
}

/// package không thấy local/global
pub fn pkg_not_found_local(package: &str) -> Error {
    anyhow!(
        "package '{package}' not found in local node_modules or MegaGate global package roots.\n\
         Use 'mg install' to install it first, provide a local path, or configure MEGAGATE_GLOBAL_PACKAGE_ROOT."
    )
}

// ===== install/app.rs =====

pub fn app_not_available(reason: &str) -> Error {
    anyhow!("'app' {reason}")
}

pub fn app_project_not_detected() -> Error {
    anyhow!(
        "Cannot detect an app project here (missing mg.toml [app] language / pubspec.yaml / build.gradle/.kts / Package.swift)."
    )
}

pub fn no_app_language(root: &std::path::Path) -> Error {
    anyhow!("No app language detected in {}", root.display())
}

pub fn app_tool_failed(cmd: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!(
        "{cmd} failed: {e} — install the tool first and ensure it is in PATH"
    )
}

/// verb không có CLI passthrough cho language
pub fn manifest_hint(verb: &str, lang: &mg_app_adapter::AppLanguage, file: &str) -> Error {
    anyhow!(
        "'{verb}' for {lang:?} has no CLI passthrough — edit {file} then run `mg install`."
    )
}

pub fn xcode_project_missing(root: &std::path::Path) -> Error {
    anyhow!(
        "no *.xcworkspace found/*.xcodeproj under {} — objC app missing an Xcode project",
        root.display()
    )
}

pub fn xcode_project_missing_short() -> Error {
    anyhow!("no *.xcworkspace found/*.xcodeproj — objC app missing an Xcode project")
}

pub fn objc_dev_needs_xcode(proj: &str) -> Error {
    anyhow!(
        "objC dev runs in Xcode — open {proj}, or set [app] dev_scheme = \"<scheme>\" in mg.toml so `mg dev` runs xcodebuild build on the simulator"
    )
}

// ===== install/ai.rs =====

pub fn ai_no_lockfile() -> Error {
    anyhow!(
        "no lock file (uv.lock or requirements.lock) — run `mg add <pkg>` to create a lock first"
    )
}

// ===== shared.rs =====

pub fn frozen_lock_missing(cmd: &str) -> Error {
    anyhow!(
        "--frozen: mg.lock is missing or does not match package.json.\n\
         Run '{cmd}' to generate an up-to-date lockfile."
    )
}

pub fn audit_strict_web_only(name: &str) -> Error {
    anyhow!(
        "--audit-strict is only implemented for the web core right now; refusing to claim policy parity for '{name}'"
    )
}

pub fn audit_strict_no_web_adapter() -> Error {
    anyhow!(
        "--audit-strict requires the web core registry adapter, which is not included in this build"
    )
}

pub fn audit_parse_time_failed(name: &str, version: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!(
        "failed to parse published time for '{name}@{version}': {e}"
    )
}

pub fn audit_recent_blocked(name: &str, version: &str, published_at: &str) -> Error {
    anyhow!(
        "--audit-strict blocked '{name}@{version}' because it was published within the last 24 hours ({published_at})"
    )
}

pub fn audit_api_status(status: &reqwest::StatusCode) -> Error {
    anyhow!("--audit-strict advisory API returned {status}")
}

pub fn audit_advisory_blocked(pkg: &str, severity: &str, title: &str) -> Error {
    anyhow!(
        "--audit-strict blocked '{pkg}' due to {severity} advisory: {title}"
    )
}

pub fn link_web_only() -> Error {
    anyhow!("link only supported for web core")
}

pub fn link_usage() -> Error {
    anyhow!("Usage: mg link <package>")
}

pub fn link_exists(name: &str) -> Error {
    anyhow!("{name} already exists in node_modules")
}

pub fn unlink_web_only() -> Error {
    anyhow!("unlink only supported for web core")
}

pub fn unlink_usage() -> Error {
    anyhow!("Usage: mg unlink <package>")
}

pub fn unlink_not_linked(name: &str) -> Error {
    anyhow!("{name} is not linked in node_modules")
}

pub fn why_web_only() -> Error {
    anyhow!("why only supported for web core")
}

pub fn lock_missing_install() -> Error {
    anyhow!("mg.lock not found — run 'mg install' first")
}

pub fn local_path_not_found(path: &std::path::Path) -> Error {
    anyhow!("local package path not found: {}", path.display())
}

pub fn cargo_toml_no_deps() -> Error {
    anyhow!("root Cargo.toml missing [dependencies]")
}

pub fn ai_project_not_detected() -> Error {
    anyhow!(
        "Cannot detect an ai project here (missing mg.toml [ai] framework / pyproject [tool.megagate] framework)."
    )
}

pub fn no_ai_framework(root: &std::path::Path) -> Error {
    anyhow!("No ai framework detected in {}", root.display())
}

pub fn python3_failed(e: &dyn std::fmt::Display) -> Error {
    anyhow!(
        "python3 failed: {e} — install Python 3.11+ and ensure `python3` is in PATH"
    )
}

// ===== dev.rs =====

pub fn path_not_utf8() -> Error {
    anyhow!("project path is not valid UTF-8")
}

pub fn unreal_editor_dev(uproject: &str) -> Error {
    anyhow!(
        "unreal dev runs in Unreal Editor (engine binary is named by version, no standard PATH) — open {uproject} in the editor, or install UnrealBuildTool and run the build directly. Run `mg build` + the editor to run."
    )
}

pub fn dev_no_command_for_engine(engine: &str) -> Error {
    anyhow!(
        "'mg dev' has no command for engine '{engine}' — use the engine editor to run it"
    )
}

pub fn godot_failed(e: &dyn std::fmt::Display) -> Error {
    anyhow!(
        "godot failed: {e} — install the Godot editor first (https://godotengine.org) and ensure `godot` is in PATH"
    )
}

pub fn espflash_failed(e: &dyn std::fmt::Display) -> Error {
    anyhow!(
        "espflash failed: {e} — install espflash first (`cargo install espflash`) and ensure it is in PATH"
    )
}

// ===== dev/cicd.rs =====

pub fn cicd_reason(reason: &str) -> Error {
    anyhow!("'cicd' {reason}")
}

pub fn ci_template_unknown(provider: &str) -> Error {
    anyhow!(
        "'mg ci generate' has no template for provider '{provider}' — supported: github-actions | gitlab | circleci"
    )
}

pub fn cicd_project_not_detected() -> Error {
    anyhow!(
        "Cannot detect a cicd project here (missing mg.toml [cicd] provider / wrangler.toml / argocd / .github/workflows)."
    )
}

// ===== dev/clo.rs =====

pub fn deploy_not_implemented(cloud: &str) -> Error {
    anyhow!("'mg deploy' for '{cloud}' cloud type is not implemented yet")
}

pub fn tool_not_installed_project(tool: &str) -> Error {
    anyhow!(
        "{tool} is not installed in the project — run `mg install` first (tools resolve from node_modules/.bin/{tool})"
    )
}

pub fn project_tool_failed(tool: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!(
        "{tool} failed: {e} — run `mg install` in this project first (tools install via node_modules/.bin)"
    )
}

// ===== dev/iot.rs =====

pub fn no_board_specified(boards: &str) -> Error {
    anyhow!(
        "No board specified — add `[iot] board = \"<id>\"` to mg.toml (known boards: {boards}) or pass `mg flash --board <id>`"
    )
}

pub fn unsupported_board(board: &str, boards: &str) -> Error {
    anyhow!("Unsupported board '{board}' — known boards: {boards}")
}

pub fn fw_path_not_utf8(elf: &std::path::Path) -> Error {
    anyhow!("firmware path is not valid UTF-8: {}", elf.display())
}

// ===== dev/app.rs =====

pub fn multi_dev_flutter_only() -> Error {
    anyhow!("multi dev P1 targets flutter/ entry — run `mg build` for other platforms")
}

// ===== create/hardware.rs =====

pub fn unknown_hardware_framework(fw: &str) -> Error {
    anyhow!(
        "unknown hardware framework '{fw}' (optimizer | bench) — templates materialize directly into the project; no separate project scaffolding"
    )
}

pub fn no_hardware_framework() -> Error {
    anyhow!("no hardware framework selected")
}

// ===== build.rs =====

pub fn no_project_found_build() -> Error {
    anyhow!("No project found (no mg.toml, package.json, or Cargo.toml)")
}

pub fn join_paths(err: &std::env::JoinPathsError) -> Error {
    anyhow!("{err}")
}

// ===== model/mod.rs =====

pub fn invalid_oci_source(oci: &str) -> Error {
    anyhow!("invalid oci source '{oci}' — use `oci://registry/repo:tag`")
}

pub fn unsupported_quantize_target(target: &str) -> Error {
    anyhow!(
        "unsupported target: {target} (use q4_k_m or q8_0; awq needs a GPU toolchain)"
    )
}

pub fn llama_cpp_missing() -> Error {
    anyhow!(
        "llama_cpp is not installed — try: `uv pip install llama-cpp-python` and rerun (A4: passthrough, does not bundle llama-cpp-2)"
    )
}

pub fn llama_quantize_failed(code: Option<i32>) -> Error {
    anyhow!("llama_cpp.quantize fail (exit {code:?})")
}

// ===== audit =====

pub fn audit_not_implemented(core: &str) -> Error {
    anyhow!(
        "{core} audit is not implemented yet; refusing to report a fake clean audit"
    )
}

pub fn audit_fix_web_only() -> Error {
    anyhow!("audit --fix is only supported for the web core")
}

pub fn web_audit_needs_context() -> Error {
    anyhow!("web audit requires project adapter context")
}

// ===== run.rs =====

pub fn script_not_found(script: &str) -> Error {
    anyhow!(
        "Script '{script}' not found. Define it in 'mg.toml' under [scripts] or in 'package.json'."
    )
}

pub fn forbidden_pm_script(cmd: &str, manifest: &std::path::Path, pm: &str) -> Error {
    anyhow!(
        "Script '{cmd}' in '{}' delegates to forbidden package manager '{pm}'. Core-web task execution must not bounce through another package manager.",
        manifest.display()
    )
}

pub fn unsupported_script(script: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!("Unsupported script '{script}': {e}")
}

// ===== dlx.rs =====

pub fn dlx_no_binary(bin: &str, pkg: &str) -> Error {
    anyhow!(
        "dlx: no binary '{bin}' found for package '{pkg}'. Is it an executable package?"
    )
}

// ===== hooks.rs =====

pub fn hook_failed(event: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!("hook {event} failed: {e}")
}

// ===== template.rs =====

pub fn registry_missing_field(field: &str) -> Error {
    anyhow!("Registry response missing {field}")
}

// ===== registry/login =====

pub fn no_token_in_response(text: &str) -> Error {
    anyhow!("No token in response: {text}")
}

pub fn no_home_dir() -> Error {
    anyhow!("No home directory")
}

// ===== info.rs =====

pub fn info_no_web_adapter() -> Error {
    anyhow!(
        "package info currently requires the web core registry adapter, which is not included in this build"
    )
}

pub fn registry_fetch_failed(package: &str, e: &dyn std::fmt::Display) -> Error {
    let _ = package;
    anyhow!("Could not fetch package info from registry: {e}")
}

// ===== outdated.rs =====

pub fn outdated_no_web_adapter() -> Error {
    anyhow!(
        "outdated currently requires the web core registry adapter, which is not included in this build"
    )
}

// ===== web_registry_config.rs =====

pub fn registry_url_https() -> Error {
    anyhow!("registry URL must use HTTPS unless it targets localhost")
}

// ===== web.rs =====

pub fn topo_order_failed(e: &dyn std::fmt::Display) -> Error {
    anyhow!("monorepo topo order failed: {e}")
}

pub fn install_slot_failed(e: &dyn std::fmt::Display) -> Error {
    anyhow!("failed to acquire install slot: {e}")
}

pub fn install_task_failed(e: &dyn std::fmt::Display) -> Error {
    anyhow!("monorepo install task failed: {e}")
}

pub fn no_framework_specified() -> Error {
    anyhow!(
        "No framework specified. Use a flag like --react, --next, --vue, or pass a framework name as the first argument."
    )
}

pub fn pm_not_supported(pm: &str) -> Error {
    anyhow!(
        "--pm {pm} is not supported by core-web. MegaGate web uses the native mg installer, not npm/pnpm/yarn/bun."
    )
}

pub fn ts_js_exclusive() -> Error {
    anyhow!("--ts and --js are mutually exclusive")
}

pub fn multiple_frameworks() -> Error {
    anyhow!(
        "Multiple frontend frameworks specified. Choose only one (--react, --next, --vue, etc.)."
    )
}

pub fn pinia_requires_vue() -> Error {
    anyhow!("--pinia requires --vue or --nuxt")
}

pub fn no_primary_package(fw: &str) -> Error {
    anyhow!("framework '{fw}' does not declare a primary package")
}

pub fn network_error_fetching(package: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!("network error fetching '{package}': {e}")
}

pub fn npm_registry_status(package: &str, status: &reqwest::StatusCode) -> Error {
    anyhow!("npm registry returned {status} for '{package}'")
}

pub fn bad_npm_response(package: &str, e: &dyn std::fmt::Display) -> Error {
    anyhow!("bad npm response for '{package}': {e}")
}

pub fn no_version_field(package: &str) -> Error {
    anyhow!("no version field for '{package}'")
}

pub fn package_json_root_object() -> Error {
    anyhow!("package.json root must be an object")
}

/// dev chưa implement cho core này
pub fn dev_core_not_implemented(core: &str) -> Error {
    anyhow!("'mg dev' Engine is not implemented for the '{core}' core yet")
}

/// dev chưa implement cho iot framework này
pub fn dev_iot_framework_not_implemented(fw: &str) -> Error {
    anyhow!("'mg dev' for '{fw}' iot framework is not implemented yet")
}

/// dev chưa implement cho cloud type này
pub fn dev_cloud_not_implemented(cloud: &str) -> Error {
    anyhow!(
        "'mg dev' for '{cloud}' cloud type is not implemented yet — supported: terraform (plan) | cdk (synth) | pulumi (preview)"
    )
}

// ===== web.rs (dev/install/script) =====

pub fn web_no_dev_target(root: &std::path::Path, hint: &str) -> Error {
    anyhow!(
        "No runnable dev target found in '{}'. Run '{}' first.",
        root.display(),
        hint
    )
}

pub fn web_workspace_dep_missing(dep: &str, target: &std::path::Path, root: &std::path::Path) -> Error {
    anyhow!(
        "workspace dependency '{dep}' referenced by '{}' was not found under '{}'",
        target.display(),
        root.display()
    )
}

pub fn web_empty_dev_script(root: &std::path::Path) -> Error {
    anyhow!("Empty dev script in '{}'", root.join("package.json").display())
}

pub fn web_unsupported_dev_script(script: &str, root: &std::path::Path) -> Error {
    anyhow!(
        "Unsupported dev script '{script}' in '{}'",
        root.join("package.json").display()
    )
}

pub fn web_forbidden_pm(script: &str, manifest: &std::path::Path, pm: &str) -> Error {
    anyhow!(
        "Unsupported script '{script}' in '{}': it delegates to '{pm}'. Core-web must execute natively through MegaGate or framework-local binaries, not through another package manager.",
        manifest.display()
    )
}

pub fn web_no_dev_entrypoint(root: &std::path::Path) -> Error {
    anyhow!(
        "No 'dev' script found in '{}' and no supported native dev entrypoint was detected",
        root.display()
    )
}

pub fn web_no_install_flow(root: &std::path::Path) -> Error {
    anyhow!(
        "No supported native install flow found in '{}'",
        root.display()
    )
}

pub fn web_missing_executable(bin: &str, hint: &str, root: &std::path::Path) -> Error {
    anyhow!(
        "Missing local executable '{bin}'. Run '{hint}' in '{}'.",
        root.display()
    )
}

// ===== build.rs / install.rs =====

pub fn build_toolchain_missing(tool: &str) -> Error {
    anyhow!("'{tool}' not found in PATH — install it first (mg doctor check lists required toolchains)")
}

pub fn workspace_failed(count: usize) -> Error {
    anyhow!("{count} workspace(s) failed to install")
}
