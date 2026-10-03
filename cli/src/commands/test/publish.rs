//! Publish lifecycle policy regressions — hồi quy policy lifecycle khi publish.

use super::{
    GitReadQuery, OCI_UPLOAD_CHUNK_BYTES, effective_trusted_oci_repository, git_checks,
    git_exec_options, parse_git_behind_count, publish_lifecycle_decision,
    publish_trusted_oci_artifacts, trusted_oci_artifact_manifest,
};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[test]
fn publish_args_read_registry_from_mgc_registry_env() {
    use clap::CommandFactory as _;

    let command = crate::Cli::command();
    let publish = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "publish")
        .expect("publish subcommand");
    let registry = publish
        .get_arguments()
        .find(|argument| argument.get_id() == "registry")
        .expect("publish registry argument");
    assert_eq!(
        registry.get_env().and_then(|value| value.to_str()),
        Some("MGC_REGISTRY")
    );
}

#[test]
fn trusted_oci_repository_uses_canonical_core_namespace() {
    assert_eq!(
        effective_trusted_oci_repository(Some("ai"), "models/weights").unwrap(),
        "ai/models/weights"
    );
    assert_eq!(
        effective_trusted_oci_repository(Some("clo"), "artifacts/app").unwrap(),
        "cloud/artifacts/app"
    );
    assert_eq!(
        effective_trusted_oci_repository(None, "acme/legacy-image").unwrap(),
        "acme/legacy-image"
    );
    assert!(effective_trusted_oci_repository(Some("ai"), "../escape").is_err());
    for core in [
        "web", "ai", "app", "lib", "game", "iot", "cloud", "cicd", "hardware",
    ] {
        assert_eq!(
            effective_trusted_oci_repository(Some(core), "pkg/artifact").unwrap(),
            format!("{core}/pkg/artifact")
        );
    }
}

#[test]
fn trusted_token_refresh_margin_and_docker_push_deadline_are_bounded() {
    let now = tokio::time::Instant::now();
    assert!(super::trusted_token_expires_within(
        now,
        now + std::time::Duration::from_secs(59),
        std::time::Duration::from_secs(60)
    ));
    assert!(!super::trusted_token_expires_within(
        now,
        now + std::time::Duration::from_secs(61),
        std::time::Duration::from_secs(60)
    ));
    assert_eq!(
        super::docker_push_timeout(std::time::Duration::from_secs(900)).unwrap(),
        std::time::Duration::from_secs(895)
    );
    assert_eq!(
        super::oci_request_timeout(std::time::Duration::from_secs(900)).unwrap(),
        std::time::Duration::from_secs(895)
    );
    assert!(super::docker_push_timeout(std::time::Duration::from_secs(5)).is_err());
    assert!(super::oci_request_timeout(std::time::Duration::from_secs(5)).is_err());
    assert_eq!(
        super::trusted_npm_request_timeout(std::time::Duration::from_secs(900)).unwrap(),
        std::time::Duration::from_secs(600)
    );
    assert_eq!(
        super::trusted_npm_request_timeout(std::time::Duration::from_secs(604)).unwrap(),
        std::time::Duration::from_secs(599)
    );
    assert!(super::trusted_npm_request_timeout(std::time::Duration::from_secs(5)).is_err());
}

#[test]
fn trusted_token_deadline_counts_exchange_network_time_against_ttl() {
    let exchange_started = tokio::time::Instant::now();
    let response_received = exchange_started + std::time::Duration::from_secs(14);
    let deadline =
        super::trusted_token_deadline(exchange_started, std::time::Duration::from_secs(900));

    assert_eq!(
        deadline,
        exchange_started + std::time::Duration::from_secs(900)
    );
    assert_eq!(
        deadline.saturating_duration_since(response_received),
        std::time::Duration::from_secs(886)
    );
}

#[test]
fn oci_upload_location_rejects_other_origins_and_unsafe_sessions() {
    let registry = url::Url::parse("https://registry.example").unwrap();
    assert!(
        super::safe_oci_upload_location(
            &registry,
            "ai/models/weights",
            "/v2/ai/models/weights/blobs/uploads/session-123?state=opaque"
        )
        .is_ok()
    );
    for location in [
        "https://attacker.example/v2/ai/models/weights/blobs/uploads/session",
        "/v2/other/package/blobs/uploads/session",
        "/v2/ai/models/weights/blobs/uploads/%2e%2e",
        "/v2/ai/models/weights/blobs/uploads/session/extra",
    ] {
        assert!(super::safe_oci_upload_location(&registry, "ai/models/weights", location).is_err());
    }
}

#[test]
fn trusted_oci_artifact_manifest_records_core_package_and_layer_digest() {
    let manifest = trusted_oci_artifact_manifest(
        "ai",
        "ai/models/weights",
        "1.0.0",
        &[(
            "weights.bin".to_owned(),
            format!("sha256:{}", "ab".repeat(32)),
            42,
        )],
    )
    .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(manifest["schemaVersion"], 2);
    assert_eq!(
        manifest["mediaType"],
        "application/vnd.oci.image.manifest.v1+json"
    );
    assert_eq!(
        manifest["artifactType"],
        "application/vnd.magicore.artifact.v1"
    );
    assert_eq!(manifest["annotations"]["io.magicore.core"], "ai");
    assert_eq!(
        manifest["annotations"]["io.magicore.package"],
        "ai/models/weights"
    );
    assert_eq!(manifest["annotations"]["io.magicore.version"], "1.0.0");
    assert_eq!(
        manifest["layers"][0]["digest"],
        format!("sha256:{}", "ab".repeat(32))
    );
    assert_eq!(manifest["layers"][0]["size"], 42);
    assert_eq!(
        manifest["layers"][0]["annotations"]["org.opencontainers.image.title"],
        "weights.bin"
    );
    assert!(trusted_oci_artifact_manifest("web", "ai/models/weights", "1.0.0", &[]).is_err());
}

#[test]
fn publish_git_queries_are_a_closed_read_only_set() {
    assert_eq!(
        GitReadQuery::ValidateBranch("release".to_string()).argv(),
        ["check-ref-format", "refs/heads/release"]
    );
    assert_eq!(
        GitReadQuery::WorkingTreeStatus.argv(),
        ["status", "--porcelain"]
    );
    assert_eq!(
        GitReadQuery::CurrentBranch.argv(),
        [
            "rev-parse",
            "--verify",
            "--abbrev-ref",
            "--end-of-options",
            "HEAD"
        ]
    );
    assert_eq!(
        GitReadQuery::UpstreamRef("release".to_string()).argv(),
        [
            "rev-parse",
            "--verify",
            "--abbrev-ref",
            "--symbolic-full-name",
            "--end-of-options",
            "release@{upstream}"
        ]
    );
    assert_eq!(
        GitReadQuery::CommitsBehind("release".to_string()).argv(),
        [
            "rev-list",
            "--count",
            "--end-of-options",
            "release..release@{upstream}"
        ]
    );
}

#[test]
fn publish_git_executor_is_locked_to_the_selected_project_root() {
    let project_root = Path::new("/tmp/magicore-publish-project");
    let options = git_exec_options(project_root);
    assert_eq!(options.cwd.as_deref(), Some(project_root));
    assert!(options.clean_env);
}

#[test]
fn publish_git_lag_parser_fails_closed_on_invalid_output() {
    assert_eq!(parse_git_behind_count("2\n").unwrap(), 2);
    assert!(parse_git_behind_count("not-a-count").is_err());
}

#[test]
fn publish_git_checks_use_selected_repo_and_reject_a_lagging_branch() {
    let temp = tempfile::tempdir().expect("temporary git test directory");
    let project_root = temp.path().join("project");
    let remote = temp.path().join("origin.git");
    let advance = temp.path().join("advance");
    let global_config = temp.path().join("gitconfig");
    fs::create_dir_all(&project_root).expect("create project");
    fs::write(&global_config, "").expect("create isolated git config");

    let run_git = |cwd: &Path, args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &global_config)
            .output()
            .expect("run local git fixture command");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run_git(&project_root, &["init", "--quiet"]);
    run_git(&project_root, &["config", "user.name", "MagiCore Test"]);
    run_git(
        &project_root,
        &["config", "user.email", "test@example.invalid"],
    );
    fs::write(project_root.join("tracked.txt"), "initial\n").expect("write tracked file");
    run_git(&project_root, &["add", "tracked.txt"]);
    run_git(&project_root, &["commit", "--quiet", "-m", "initial"]);
    run_git(&project_root, &["branch", "-M", "publish-check"]);

    let remote_arg = remote.to_string_lossy().into_owned();
    run_git(
        &project_root,
        &["clone", "--quiet", "--bare", ".", &remote_arg],
    );
    run_git(&project_root, &["remote", "add", "origin", &remote_arg]);
    run_git(&project_root, &["fetch", "--quiet", "origin"]);
    run_git(
        &project_root,
        &[
            "branch",
            "--set-upstream-to=origin/publish-check",
            "publish-check",
        ],
    );
    git_checks(Some("publish-check"), &project_root)
        .expect("selected project's branch should initially match upstream");
    assert!(git_checks(Some("bad/branch?"), &project_root).is_err());

    let advance_arg = advance.to_string_lossy().into_owned();
    run_git(
        temp.path(),
        &["clone", "--quiet", &remote_arg, &advance_arg],
    );
    run_git(&advance, &["config", "user.name", "MagiCore Test"]);
    run_git(&advance, &["config", "user.email", "test@example.invalid"]);
    fs::write(advance.join("tracked.txt"), "remote update\n").expect("update remote file");
    run_git(&advance, &["add", "tracked.txt"]);
    run_git(&advance, &["commit", "--quiet", "-m", "remote update"]);
    run_git(&advance, &["push", "--quiet", "origin", "publish-check"]);
    run_git(&project_root, &["fetch", "--quiet", "origin"]);

    let error = git_checks(Some("publish-check"), &project_root)
        .expect_err("publish must reject a selected branch behind its upstream");
    assert!(error.to_string().contains("lags 1 commits behind upstream"));
}

#[test]
fn publish_hooks_require_explicit_opt_in_by_default() {
    let package = json!({"scripts": {"prepare": "node build.js"}});
    let error = publish_lifecycle_decision(&package, false, false)
        .expect_err("publish hooks must be blocked without explicit approval");
    assert!(error.to_string().contains("--allow-scripts"));
}

#[test]
fn publish_hooks_run_after_explicit_opt_in() {
    let package = json!({"scripts": {"prepublishOnly": "node build.js"}});
    assert!(
        publish_lifecycle_decision(&package, false, true)
            .expect("explicit opt-in should allow supported publish hooks")
    );
}

#[test]
fn ignore_scripts_skips_publish_hooks() {
    let package = json!({"scripts": {"prepare": "node build.js"}});
    assert!(
        !publish_lifecycle_decision(&package, true, false)
            .expect("ignore-scripts should skip lifecycle execution")
    );
}

#[test]
fn packages_without_publish_hooks_need_no_opt_in() {
    let package = json!({"scripts": {"test": "node test.js"}});
    assert!(
        !publish_lifecycle_decision(&package, false, false)
            .expect("unrelated scripts should not gate publishing")
    );
}

#[test]
fn malformed_publish_lifecycle_metadata_fails_closed() {
    let package = json!({"scripts": {"prepare": false}});
    assert!(publish_lifecycle_decision(&package, false, true).is_err());
}

#[derive(Default)]
struct MockOciRegistry {
    next_upload: u64,
    uploads: HashMap<String, (String, Vec<u8>)>,
    blobs: HashMap<(String, String), Vec<u8>>,
    manifest: Option<(String, String, serde_json::Value)>,
}

async fn mock_oci_registry(
    State(state): State<Arc<Mutex<MockOciRegistry>>>,
    AxumPath(path): AxumPath<String>,
    method: Method,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        != Some("Bearer local-token")
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let route = path.as_str();
    let mut state = state.lock().unwrap();
    if method == Method::POST {
        let Some(repository) = route.strip_suffix("/blobs/uploads/") else {
            return StatusCode::NOT_FOUND.into_response();
        };
        state.next_upload += 1;
        let id = format!("upload-{}", state.next_upload);
        state
            .uploads
            .insert(id.clone(), (repository.to_owned(), Vec::new()));
        let mut response = StatusCode::ACCEPTED.into_response();
        response.headers_mut().insert(
            header::LOCATION,
            format!("/v2/{repository}/blobs/uploads/{id}")
                .parse()
                .unwrap(),
        );
        return response;
    }
    if method == Method::PATCH {
        let Some((_repository, upload_id)) = route.split_once("/blobs/uploads/") else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Some((_, bytes)) = state.uploads.get_mut(upload_id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Some((start, end)) = headers
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split_once('-'))
        else {
            return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
        };
        let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) else {
            return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
        };
        if start != bytes.len()
            || end.checked_sub(start).and_then(|size| size.checked_add(1)) != Some(body.len())
        {
            return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
        }
        bytes.extend_from_slice(&body);
        let mut response = StatusCode::ACCEPTED.into_response();
        response
            .headers_mut()
            .insert(header::LOCATION, format!("/v2/{route}").parse().unwrap());
        response.headers_mut().insert(
            header::RANGE,
            format!("0-{}", bytes.len() - 1).parse().unwrap(),
        );
        return response;
    }
    if method == Method::PUT && route.contains("/blobs/uploads/") {
        let Some((repository, upload_id)) = route.split_once("/blobs/uploads/") else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Some(expected_digest) = query.get("digest") else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Some((_, bytes)) = state.uploads.remove(upload_id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        use sha2::Digest as _;
        let actual_digest = format!("sha256:{}", hex::encode(sha2::Sha256::digest(&bytes)));
        if &actual_digest != expected_digest {
            return StatusCode::BAD_REQUEST.into_response();
        }
        state
            .blobs
            .insert((repository.to_owned(), actual_digest), bytes);
        return StatusCode::CREATED.into_response();
    }
    if method == Method::PUT {
        let Some((repository, version)) = route.split_once("/manifests/") else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&body) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let config_digest = manifest["config"]["digest"].as_str().unwrap_or_default();
        if !state
            .blobs
            .contains_key(&(repository.to_owned(), config_digest.to_owned()))
        {
            return StatusCode::BAD_REQUEST.into_response();
        }
        if let Some(layers) = manifest["layers"].as_array() {
            for layer in layers {
                let digest = layer["digest"].as_str().unwrap_or_default();
                if !state
                    .blobs
                    .contains_key(&(repository.to_owned(), digest.to_owned()))
                {
                    return StatusCode::BAD_REQUEST.into_response();
                }
            }
        } else {
            return StatusCode::BAD_REQUEST.into_response();
        }
        state.manifest = Some((repository.to_owned(), version.to_owned(), manifest));
        return StatusCode::CREATED.into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

#[tokio::test]
async fn trusted_oci_artifact_upload_streams_blobs_and_publishes_manifest() {
    use sha2::Digest as _;

    let state = Arc::new(Mutex::new(MockOciRegistry::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/v2/*path", any(mock_oci_registry))
        .with_state(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let temp = tempfile::tempdir().unwrap();
    let artifact = temp.path().join("model.bin");
    let bytes = vec![0x5a; OCI_UPLOAD_CHUNK_BYTES * 2 + 17];
    fs::write(&artifact, &bytes).unwrap();
    let registry = format!("http://{address}");
    let mut session = super::TrustedPublishSession {
        token: "local-token".into(),
        expires_at: tokio::time::Instant::now() + std::time::Duration::from_secs(900),
        registry: registry.clone(),
        package: "ai/models/weights".into(),
        protocol: "oci".into(),
        audience: None,
    };
    let result = publish_trusted_oci_artifacts(
        &registry,
        "ai/models/weights",
        "ai",
        "1.0.0",
        &[artifact],
        &mut session,
    )
    .await;
    server.abort();
    result.unwrap();

    let state = state.lock().unwrap();
    assert_eq!(state.blobs.len(), 2);
    let (repository, version, manifest) = state.manifest.as_ref().unwrap();
    assert_eq!(repository, "ai/models/weights");
    assert_eq!(version, "1.0.0");
    assert_eq!(manifest["annotations"]["io.magicore.core"], "ai");
    assert_eq!(manifest["layers"][0]["size"], bytes.len() as u64);
    assert_eq!(
        manifest["layers"][0]["digest"],
        format!("sha256:{}", hex::encode(sha2::Sha256::digest(&bytes)))
    );
}
