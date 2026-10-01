//! Model command — mgc model push/pull (10-task-plan Phase 3)
//! (Lệnh model: push model qua OCI registry, pull về máy)
//!
//! AI core (Q11): `mgc model pull hf://org/model/file` hoặc `oci://registry/repo:tag`
//! → CAS store (`~/.magicore/store/v3`, T1) + manifest model; list/rm local.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use mgc_oci::client::OciClient;
use mgc_oci::manifest::OciImageConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Default local registry URL (RULE §13: port chứa 4·3·1·5)
const DEFAULT_REGISTRY: &str = "http://127.0.0.1:4315";

const MODEL_MEDIA_TYPE: &str = "application/vnd.magicore.model.layer.v1+file";
/// Default guard for remote model pulls; larger artifacts require an explicit limit.
const DEFAULT_MODEL_PULL_MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/* ─── Local model manifest (CAS AI core, Q11) ─────────────────────── */

fn store_root() -> PathBuf {
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(root) = std::env::var("MAGICORE_STORE_ROOT")
        && !root.is_empty()
    {
        return PathBuf::from(root);
    }
    mgc_store::default_store_root()
}

fn model_manifest_dir() -> PathBuf {
    store_root().join("models")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ModelManifest {
    name: String,
    source: String,
    blobs: Vec<String>,
    total_bytes: u64,
    pulled_at: String,
}

fn model_manifest_path(name: &str) -> Result<PathBuf> {
    let dir = model_manifest_dir();
    Ok(dir.join(existing_or_encoded_manifest_path(&dir, name)?))
}

fn model_manifest_relative_path(name: &str) -> Result<PathBuf> {
    validate_model_name(name)?;
    let mut parts = name.split('/').collect::<Vec<_>>();
    let leaf = parts
        .pop()
        .ok_or_else(|| crate::error::invalid_model_name(name))?;
    let mut relative = PathBuf::new();
    for part in parts {
        relative.push(encode_model_path_segment(part));
    }
    relative.push(format!("{}.json", encode_model_path_segment(leaf)));
    Ok(relative)
}

fn existing_or_encoded_manifest_path(dir: &Path, name: &str) -> Result<PathBuf> {
    let encoded = model_manifest_relative_path(name)?;
    let mut legacy_parts = name.split('/').collect::<Vec<_>>();
    let leaf = legacy_parts
        .pop()
        .ok_or_else(|| crate::error::invalid_model_name(name))?;
    let mut legacy = PathBuf::new();
    for part in legacy_parts {
        legacy.push(part);
    }
    legacy.push(format!("{leaf}.json"));
    if legacy == encoded {
        return Ok(encoded);
    }
    match std::fs::symlink_metadata(dir.join(&legacy)) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(legacy),
        Ok(_) => Err(crate::error::unsafe_model_manifest_path()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(encoded),
        Err(error) => Err(error.into()),
    }
}

fn encode_model_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn validate_model_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains(['\\', '\0'])
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(name).is_absolute()
    {
        return Err(crate::error::invalid_model_name(name));
    }
    Ok(())
}

fn save_manifest(m: &ModelManifest) -> Result<()> {
    save_manifest_in(model_manifest_dir(), m)
}

fn save_manifest_in(dir: PathBuf, m: &ModelManifest) -> Result<()> {
    let relative = existing_or_encoded_manifest_path(&dir, &m.name)?;
    std::fs::create_dir_all(&dir)?;
    if !std::fs::symlink_metadata(&dir)?.file_type().is_dir() {
        return Err(crate::error::unsafe_model_manifest_path());
    }
    let mut parent = dir.clone();
    for component in relative.parent().into_iter().flat_map(Path::components) {
        parent.push(component);
        match std::fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(crate::error::unsafe_model_manifest_path()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::create_dir(&parent) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        if !std::fs::symlink_metadata(&parent)?.file_type().is_dir() {
                            return Err(crate::error::unsafe_model_manifest_path());
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    let path = dir.join(relative);
    let staging = path.with_file_name(format!(
        ".{}.mgc-manifest-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model.json"),
        uuid::Uuid::new_v4()
    ));
    let mut staging_created = false;
    let result = (|| -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)?;
        staging_created = true;
        file.write_all(&serde_json::to_vec_pretty(m)?)?;
        file.sync_all()?;
        drop(file);
        mgc_lockfile::atomic::atomic_replace_file(&staging, &path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() && staging_created {
        let _ = std::fs::remove_file(&staging);
    }
    result
}

fn read_manifests_in(dir: PathBuf) -> Vec<ModelManifest> {
    let mut out = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        if !std::fs::symlink_metadata(&d).is_ok_and(|metadata| metadata.file_type().is_dir()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let Ok(metadata) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if metadata.file_type().is_dir() {
                stack.push(p);
            } else if metadata.file_type().is_file() && p.extension().is_some_and(|x| x == "json") {
                // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
                if let Ok(s) = std::fs::read_to_string(&p)
                    && let Ok(m) = serde_json::from_str(&s)
                {
                    out.push(m);
                }
            }
        }
    }
    out
}

fn now_iso() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// CAS pull — tải nguồn ngoài (HF/OCI) vào CAS store + manifest.
async fn cas_pull(
    source: &str,
    revision: Option<&str>,
    sha256: Option<&str>,
    max_bytes: Option<u64>,
) -> Result<()> {
    let store = mgc_store::cas::ContentStore::new(store_root())?;
    let (name, blobs, total, recorded_source) = if let Some(hf) = source.strip_prefix("hf://") {
        let max_bytes = max_bytes.unwrap_or(DEFAULT_MODEL_PULL_MAX_BYTES);
        if max_bytes == 0 {
            bail!("--max-bytes must be greater than zero");
        }
        let (name, blobs, total, pinned_source) =
            pull_hf(&store, hf, revision, sha256, max_bytes).await?;
        (name, blobs, total, pinned_source)
    } else if let Some(oci) = source.strip_prefix("oci://") {
        if revision.is_some() || sha256.is_some() || max_bytes.is_some() {
            return Err(crate::error::model_pull_flags_require_hf());
        }
        let (name, blobs, total) = pull_oci(&store, oci).await?;
        (name, blobs, total, source.to_string())
    } else {
        bail!(
            "unsupported model source '{source}' — use `hf://org/model/file` or `oci://registry/repo:tag`"
        );
    };

    let manifest = ModelManifest {
        name,
        source: recorded_source,
        total_bytes: total,
        pulled_at: now_iso(),
        blobs,
    };
    save_manifest(&manifest)?;
    println!(
        "pulled {} → CAS ({} bytes, manifest at {})",
        manifest.name,
        manifest.total_bytes,
        model_manifest_path(&manifest.name)?.display()
    );
    Ok(())
}

fn cas_import(store: &mgc_store::cas::ContentStore, src: &Path) -> Result<(String, u64)> {
    let len = std::fs::metadata(src)?.len();
    let hash = store.import_file(src)?;
    // Accessor (P0-1): IntegrityHash fields are private — read via as_hex().
    // (Accessor (P0-1): field IntegrityHash đã private — đọc qua as_hex().)
    Ok((hash.as_hex().to_string(), len))
}

/// Pull a revision-pinned Hugging Face artifact and verify its SHA-256 before CAS import.
/// Tải artifact Hugging Face đã ghim revision và xác minh SHA-256 trước khi nhập CAS.
async fn pull_hf(
    store: &mgc_store::cas::ContentStore,
    hf: &str,
    revision: Option<&str>,
    sha256: Option<&str>,
    max_bytes: u64,
) -> Result<(String, Vec<String>, u64, String)> {
    let (name, url, revision, expected_sha256) = parse_hf_locator(hf, revision, sha256)?;
    let response = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(2 * 60 * 60))
        .build()?
        .get(url.as_str())
        .send()
        .await
        .with_context(crate::error::hf_request_failed)?;
    if !response.status().is_success() {
        bail!(
            "HF download failed: {} ({url}) — artifact was not written to the store",
            response.status()
        );
    }
    let declared_length = response.content_length();
    if declared_length.is_some_and(|declared| declared > max_bytes) {
        bail!("HF artifact exceeds --max-bytes ({max_bytes}); no artifact was written");
    }
    let tmp = std::env::temp_dir().join(format!(
        "mgc-hf-{}-{}.part",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = async {
        use sha2::Digest;
        use tokio::io::AsyncWriteExt;

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .await?;
        let mut response = response;
        let mut hasher = sha2::Sha256::new();
        let mut total = 0_u64;
        while let Some(chunk) = response.chunk().await? {
            total = next_hf_download_total(total, chunk.len(), max_bytes)?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
        }
        if declared_length.is_some_and(|declared| declared != total) {
            return Err(crate::error::hf_content_length_mismatch(
                declared_length.unwrap_or_default(),
                total,
            ));
        }
        file.sync_all().await?;
        drop(file);
        let actual = hex::encode(hasher.finalize());
        if !actual.eq_ignore_ascii_case(&expected_sha256) {
            return Err(crate::error::hf_sha256_mismatch(&expected_sha256, &actual));
        }
        let (hash, imported_len) = cas_import(store, &tmp)?;
        if imported_len != total {
            return Err(crate::error::hf_artifact_changed_during_import());
        }
        let pinned_source = format!("hf://{hf}@{revision}#sha256={expected_sha256}");
        Ok((name, vec![hash], total, pinned_source))
    }
    .await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

fn next_hf_download_total(current: u64, chunk_len: usize, max_bytes: u64) -> Result<u64> {
    let next = current
        .checked_add(chunk_len as u64)
        .ok_or_else(crate::error::hf_size_overflow)?;
    if next > max_bytes {
        return Err(crate::error::hf_download_limit_exceeded(max_bytes));
    }
    Ok(next)
}

fn parse_hf_locator(
    hf: &str,
    revision: Option<&str>,
    sha256: Option<&str>,
) -> Result<(String, url::Url, String, String)> {
    let revision = revision.ok_or_else(crate::error::hf_revision_required)?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(crate::error::invalid_hf_revision());
    }
    let sha256 = sha256.ok_or_else(crate::error::hf_sha256_required)?;
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(crate::error::invalid_hf_sha256());
    }
    let segments = hf.split('/').collect::<Vec<_>>();
    if segments.len() < 3
        || segments.iter().any(|part| {
            part.is_empty() || matches!(*part, "." | "..") || part.contains(['\\', '\0', ':'])
        })
    {
        return Err(crate::error::invalid_hf_source());
    }
    let name = segments.join("/");
    validate_model_name(&name)?;
    let mut url = url::Url::parse("https://huggingface.co/")?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("unable to construct HF artifact URL"))?;
        path.pop_if_empty()
            .push(segments[0])
            .push(segments[1])
            .push("resolve")
            .push(revision);
        for segment in &segments[2..] {
            path.push(segment);
        }
    }
    Ok((name, url, revision.to_string(), sha256.to_ascii_lowercase()))
}

/// oci://registry/repo:tag → pull manifest + layers → CAS (P1 cơ bản).
async fn pull_oci(
    store: &mgc_store::cas::ContentStore,
    oci: &str,
) -> Result<(String, Vec<String>, u64)> {
    let (registry, rest) = oci
        .split_once('/')
        .ok_or_else(|| crate::error::invalid_oci_source(oci))?;
    let (repo, tag) = match rest.rsplit_once(':') {
        Some((r, t)) if !r.is_empty() && !t.is_empty() => (r, t),
        _ => (rest, "latest"),
    };
    let base = if registry.contains("://") {
        registry.to_string()
    } else {
        format!("http://{registry}")
    };

    let c = client(&base, None)?;
    let manifest = c
        .pull_manifest(repo, tag)
        .await
        .with_context(crate::error::pull_manifest_failed)?;

    let mut blobs = Vec::new();
    let mut total = 0u64;
    for layer in &manifest.layers {
        let data = c
            .pull_blob(repo, &layer.digest)
            .await
            .with_context(|| format!("pull blob {}", layer.digest))?;
        let tmp =
            std::env::temp_dir().join(format!("mgc-oci-{}-{}", std::process::id(), now_iso()));
        std::fs::write(&tmp, &data)?;
        let (hash, len) = cas_import(store, &tmp)?;
        let _ = std::fs::remove_file(&tmp);
        blobs.push(hash);
        total += len;
        println!(
            "  blob {} ({len} bytes)",
            layer.digest.trim_start_matches("sha256:")
        );
    }

    let name = format!("{repo}:{tag}");
    Ok((name, blobs, total))
}

/// Liệt kê model local (CAS manifest).
fn list_local() -> Result<()> {
    let manifests = read_manifests_in(model_manifest_dir());
    if manifests.is_empty() {
        println!("(no local models — pull one: `mgc model pull hf://org/model/file`)");
        return Ok(());
    }
    for m in manifests {
        println!(
            "{}
  source: {}
  {} bytes, {} blob(s), pulled {}",
            m.name,
            m.source,
            m.total_bytes,
            m.blobs.len(),
            m.pulled_at
        );
    }
    Ok(())
}

/// Xoá model local — manifest + blob CAS chỉ khi không còn manifest nào trỏ (refcount).
fn remove_local(name: &str) -> Result<()> {
    let path = model_manifest_path(name)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(crate::error::unsafe_model_manifest_path()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "model '{name}' not found locally (manifest: {})",
                path.display()
            );
        }
        Err(error) => return Err(error.into()),
    }
    let manifest: ModelManifest = serde_json::from_str(&std::fs::read_to_string(&path)?)
        .with_context(crate::error::parse_manifest_failed)?;

    let all: Vec<ModelManifest> = read_manifests_in(model_manifest_dir());
    let others: Vec<&str> = all
        .iter()
        .filter(|m| m.name != name)
        .flat_map(|m| m.blobs.iter().map(|b| b.as_str()))
        .collect();

    let store = mgc_store::cas::ContentStore::new(store_root())?;
    for blob in &manifest.blobs {
        if others.contains(&blob.as_str()) {
            continue; // còn model khác dùng — giữ blob (refcount T1)
        }
        // Manifest hash is external input — fail-closed validation, then warn
        // and skip the blob (deletion must not abort the whole removal).
        // Hash trong manifest là input ngoài — validate fail-closed, nếu sai
        // dạng thì cảnh báo và bỏ qua blob (không abort cả lệnh remove).
        let hash = match mgc_store::cas::IntegrityHash::from_hash_str(blob, false) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("warning: invalid blob hash '{blob}', skipping: {e}");
                continue;
            }
        };
        if let Err(e) = store.remove(&hash) {
            eprintln!("warning: failed to remove blob {blob}: {e}");
        }
    }
    std::fs::remove_file(&path)?;
    println!("removed model '{name}'");
    Ok(())
}

#[derive(Args, Debug, Clone)]
pub struct ModelArgs {
    #[command(subcommand)]
    pub cmd: ModelCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ModelCmd {
    /// Push model files to registry (one layer per file + manifest)
    Push {
        /// Model files/directories (directories are walked; one layer per file)
        #[arg(required = true)]
        paths: Vec<String>,
        /// Repo, e.g. ai/my-model
        #[arg(long, default_value = "ai/default")]
        repo: String,
        #[arg(long, default_value = "latest")]
        tag: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        token: Option<String>,
    },
    /// Pull model: `hf://org/model/file` / `oci://registry/repo:tag` (→ CAS store)
    /// or from the local registry (writes files as-is into the output dir).
    Pull {
        /// Repo: hf://org/model/file | oci://registry/repo:tag | ai/my-model (local registry)
        repo: String,
        #[arg(long, default_value = "latest")]
        tag: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, default_value = ".")]
        output: String,
        /// Immutable 40-hex Hugging Face commit revision (required for hf://).
        #[arg(long)]
        revision: Option<String>,
        /// Trusted SHA-256 of the exact Hugging Face artifact (required for hf://).
        #[arg(long)]
        sha256: Option<String>,
        /// Maximum Hugging Face download size in bytes (default: 64 GiB).
        #[arg(long)]
        max_bytes: Option<u64>,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        token: Option<String>,
    },
    /// List: default is the registry catalog; `--local` = models in the CAS store
    List {
        #[arg(long)]
        local: bool,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        token: Option<String>,
    },
    /// Remove a local model from the CAS store (manifest + unreferenced blobs)
    Rm {
        /// Model name (matches manifest: org/model/file or repo:tag)
        name: String,
    },
    /// Quantize GGUF (native quantizer not yet implemented).
    Quantize {
        /// Path to the source model file (GGUF/ggml)
        path: String,
        /// Target quant: q4_k_m | q8_0 (awq needs a GPU toolchain — unsupported)
        #[arg(long, default_value = "q4_k_m")]
        target: String,
        /// Output path (default: <path>.<target>.gguf in the same directory)
        #[arg(long)]
        output: Option<String>,
    },
}

pub async fn run(args: ModelArgs) -> Result<()> {
    match args.cmd {
        ModelCmd::Push {
            paths,
            repo,
            tag,
            registry,
            token,
        } => push(paths, &repo, &tag, &registry, token).await,
        ModelCmd::Pull {
            repo,
            tag,
            registry,
            output,
            revision,
            sha256,
            max_bytes,
            token,
        } => {
            if repo.starts_with("hf://") || repo.starts_with("oci://") {
                cas_pull(&repo, revision.as_deref(), sha256.as_deref(), max_bytes).await
            } else {
                if revision.is_some() || sha256.is_some() || max_bytes.is_some() {
                    return Err(crate::error::model_pull_flags_require_hf());
                }
                pull(&repo, &tag, &registry, &output, token).await
            }
        }
        ModelCmd::List {
            local,
            registry,
            token,
        } => {
            if local {
                list_local()
            } else {
                list(&registry, token).await
            }
        }
        ModelCmd::Rm { name } => remove_local(&name),
        ModelCmd::Quantize {
            path,
            target,
            output,
        } => quantize(&path, &target, output.as_deref()),
    }
}

/// Fail closed until MagiCore ships its own GGUF quantization engine.
/// Fail-closed cho tới khi MagiCore có engine lượng tử hóa GGUF riêng.
fn quantize(_path: &str, _target: &str, _output: Option<&str>) -> Result<()> {
    anyhow::bail!(
        "native GGUF quantization is not implemented; MagiCore will not invoke Python or an external quantizer"
    )
}

fn client(registry: &str, token: Option<String>) -> Result<OciClient> {
    let mut c = OciClient::new(registry.to_string(), None)?;
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        c = c.with_token(t);
    }
    Ok(c)
}

async fn push(
    paths: Vec<String>,
    repo: &str,
    tag: &str,
    registry: &str,
    token: Option<String>,
) -> Result<()> {
    let mut layers: Vec<(PathBuf, String)> = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new(); // digest -> filename

    for p in &paths {
        let path = Path::new(p);
        if !path.exists() {
            return Err(crate::error::path_not_found(std::path::Path::new(&p)));
        }
        if path.is_dir() {
            // thư mục: liệt kê file con trực tiếp (không đệ quy sâu)
            let mut files: Vec<PathBuf> = std::fs::read_dir(path)
                .with_context(|| format!("reading directory {p}"))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|f| f.is_file())
                .collect();
            files.sort();
            if files.is_empty() {
                return Err(crate::error::dir_has_no_files(std::path::Path::new(&p)));
            }
            for f in files {
                names.insert(
                    digest_of(&f).await?,
                    f.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "layer.bin".to_string()),
                );
                layers.push((f, MODEL_MEDIA_TYPE.to_string()));
            }
        } else {
            names.insert(
                digest_of(path).await?,
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "layer.bin".to_string()),
            );
            layers.push((path.to_path_buf(), MODEL_MEDIA_TYPE.to_string()));
        }
    }

    let config = OciImageConfig {
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default(),
        architecture: "any".into(),
        os: "any".into(),
        config: None,
        rootfs: None,
        history: None,
        annotations: Some(names),
    };

    let c = client(registry, token)?;
    let pushed = c
        .push_model(repo, tag, &config, &layers)
        .await
        .with_context(crate::error::push_model_failed)?;
    println!(
        "pushed {repo}:{pushed} ({registry}) — {} layer(s)",
        layers.len()
    );
    Ok(())
}

async fn digest_of(path: &Path) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        h.update(&buffer[..read]);
    }
    Ok(format!("sha256:{:x}", h.finalize()))
}

async fn pull(
    repo: &str,
    tag: &str,
    registry: &str,
    output: &str,
    token: Option<String>,
) -> Result<()> {
    let c = client(registry, token)?;
    let manifest = c
        .pull_manifest(repo, tag)
        .await
        .with_context(crate::error::parse_manifest_failed)?;

    // Tên file gốc nằm trong annotations của config blob (OciImageConfig)
    let config_data = c
        .pull_blob(repo, &manifest.config.digest)
        .await
        .with_context(crate::error::parse_manifest_failed)?;
    let config: OciImageConfig =
        serde_json::from_slice(&config_data).with_context(crate::error::parse_manifest_failed)?;
    let names: HashMap<String, String> = config.annotations.unwrap_or_default();

    let out_dir = Path::new(output);
    std::fs::create_dir_all(out_dir)?;

    let mut written = 0;
    for (i, layer) in manifest.layers.iter().enumerate() {
        let digest = layer.digest.trim_start_matches("sha256:");
        let default_name = format!("layer-{i}.bin");
        let name = names
            .get(&layer.digest)
            .map(|s| {
                Path::new(s)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| default_name.clone())
            })
            .unwrap_or(default_name);
        let data = c
            .pull_blob(repo, &layer.digest)
            .await
            .with_context(|| format!("pull blob {}", layer.digest))?;
        std::fs::write(out_dir.join(&name), &data)?;
        let _ = digest;
        written += 1;
        println!("  {name} ({} bytes)", data.len());
    }
    println!(
        "pulled {repo}:{tag} → {} ({} file)",
        out_dir.display(),
        written
    );
    Ok(())
}

async fn list(registry: &str, token: Option<String>) -> Result<()> {
    let c = client(registry, token)?;
    let repos = c
        .list_repositories()
        .await
        .with_context(crate::error::catalog_failed)?;
    for repo in repos {
        let tags = c.list_tags(&repo).await.unwrap_or_default();
        println!("{repo}: {}", tags.join(", "));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../test/model_test.rs"]
mod tests;
