//! Model command — mgc model push/pull (10-task-plan Phase 3)
//! (Lệnh model: push model qua OCI registry, pull về máy)
//!
//! AI core (Q11): `mgc model pull hf://org/model/file` hoặc `oci://registry/repo:tag`
//! → CAS store (`~/.magicore/store/v3`, T1) + manifest model; list/rm local.

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use mgc_oci::client::OciClient;
use mgc_oci::manifest::OciImageConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Default local registry URL (RULE §13: port chứa 4·3·1·5)
const DEFAULT_REGISTRY: &str = "http://127.0.0.1:4315";

const MODEL_MEDIA_TYPE: &str = "application/vnd.magicore.model.layer.v1+file";

/* ─── Local model manifest (CAS AI core, Q11) ─────────────────────── */

fn store_root() -> PathBuf {
    if let Ok(root) = std::env::var("MAGICORE_STORE_ROOT") {
        if !root.is_empty() {
            return PathBuf::from(root);
        }
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

fn model_manifest_path(name: &str) -> PathBuf {
    model_manifest_dir().join(format!("{name}.json"))
}

fn save_manifest(m: &ModelManifest) -> Result<()> {
    save_manifest_in(model_manifest_dir(), m)
}

fn save_manifest_in(dir: PathBuf, m: &ModelManifest) -> Result<()> {
    let path = dir.join(format!("{}.json", m.name));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(m)?)?;
    Ok(())
}

fn read_manifests_in(dir: PathBuf) -> Vec<ModelManifest> {
    let mut out = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "json") {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    if let Ok(m) = serde_json::from_str(&s) {
                        out.push(m);
                    }
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
async fn cas_pull(source: &str) -> Result<()> {
    let store = mgc_store::cas::ContentStore::new(store_root())?;
    let (name, blobs, total) = if let Some(hf) = source.strip_prefix("hf://") {
        pull_hf(&store, hf).await?
    } else if let Some(oci) = source.strip_prefix("oci://") {
        pull_oci(&store, oci).await?
    } else {
        bail!(
            "unsupported model source '{source}' — use `hf://org/model/file` or `oci://registry/repo:tag`"
        );
    };

    let manifest = ModelManifest {
        name,
        source: source.to_string(),
        total_bytes: total,
        pulled_at: now_iso(),
        blobs,
    };
    save_manifest(&manifest)?;
    println!(
        "pulled {} → CAS ({} bytes, manifest at {})",
        manifest.name,
        manifest.total_bytes,
        model_manifest_path(&manifest.name).display()
    );
    Ok(())
}

fn cas_import(store: &mgc_store::cas::ContentStore, src: &Path) -> Result<(String, u64)> {
    let len = std::fs::metadata(src)?.len();
    let hash = store.import_file(src)?;
    Ok((hash.hash, len))
}

/// hf://org/model/file → https://huggingface.co/{org}/{model}/resolve/main/{file}
async fn pull_hf(
    store: &mgc_store::cas::ContentStore,
    hf: &str,
) -> Result<(String, Vec<String>, u64)> {
    let mut parts = hf.split('/');
    let org = parts.next().unwrap_or_default();
    let model = parts.next().unwrap_or_default();
    let file = parts.next();
    if org.is_empty() || model.is_empty() {
        bail!("invalid hf source '{hf}' — use `hf://org/model/file`");
    }
    let Some(file) = file.filter(|f| !f.is_empty()) else {
        bail!(
            "missing model file in '{hf}' — specify a file: `hf://{org}/{model}/<file>` (branch mặc định: main)"
        );
    };

    let url = format!("https://huggingface.co/{org}/{model}/resolve/main/{file}");
    let resp = reqwest::get(&url)
        .await
        .with_context(crate::error::hf_request_failed)?;
    if !resp.status().is_success() {
        bail!(
            "HF download failed: {} ({url}) — unknown source, not written to store",
            resp.status()
        );
    }
    let data = resp.bytes().await?;

    let tmp = std::env::temp_dir().join(format!("mgc-hf-{}-{}", std::process::id(), now_iso()));
    std::fs::write(&tmp, &data)?;
    let (hash, len) = cas_import(store, &tmp)?;
    let _ = std::fs::remove_file(&tmp);

    let name = format!("{org}/{model}/{file}");
    Ok((name, vec![hash], len))
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
    let path = model_manifest_path(name);
    if !path.exists() {
        bail!(
            "model '{name}' not found locally (manifest: {})",
            path.display()
        );
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
        let hash = mgc_store::cas::IntegrityHash::from_hash_str(blob, false);
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
    /// Push model files to registry (mỗi file 1 layer + manifest)
    Push {
        /// Files/thư mục model (thư mục = nén tar? — hiện tại mỗi file 1 layer)
        #[arg(required = true)]
        paths: Vec<String>,
        /// Repo: ai/ten-model
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
    /// hoặc từ registry local (ghi file như cũ, output dir).
    Pull {
        /// Repo: hf://org/model/file | oci://registry/repo:tag | ai/ten-model (registry local)
        repo: String,
        #[arg(long, default_value = "latest")]
        tag: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, default_value = ".")]
        output: String,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        token: Option<String>,
    },
    /// List: mặc định registry catalog; `--local` = model trong CAS store
    List {
        #[arg(long)]
        local: bool,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        token: Option<String>,
    },
    /// Xoá model local khỏi CAS store (manifest + blob không ai trỏ)
    Rm {
        /// Tên model (khớp manifest: org/model/file hoặc repo:tag)
        name: String,
    },
    /// Quantize GGUF (A4: python passthrough `python -m llama_cpp.quantize` —
    /// không static link llama-cpp-2; fail-closed nếu llama_cpp chưa cài)
    Quantize {
        /// Đường dẫn file model gốc (GGUF/ggml)
        path: String,
        /// Target quant: q4_k_m | q8_0 (awq cần GPU toolchain — chưa hỗ trợ)
        #[arg(long, default_value = "q4_k_m")]
        target: String,
        /// Output path (mặc định: <path>.<target>.gguf cùng thư mục)
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
            token,
        } => {
            if repo.starts_with("hf://") || repo.starts_with("oci://") {
                cas_pull(&repo).await
            } else {
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

/// GGUF quantize qua python passthrough (A4, sys-mgc/05 §4)
fn quantize(path: &str, target: &str, output: Option<&str>) -> Result<()> {
    if target != "q4_k_m" && target != "q8_0" {
        return Err(crate::error::unsupported_quantize_target(target));
    }
    if !std::path::Path::new(path).exists() {
        return Err(crate::error::file_not_found(std::path::Path::new(path)));
    }
    let out = match output {
        Some(o) => o.to_string(),
        None => format!("{path}.{target}.gguf"),
    };
    // WARNING: TĨNH cho đường dẫn được chỉ định — KHÔNG dùng đường dẫn từ prompt
    let py = std::process::Command::new("python3")
        .args(["-c", "import llama_cpp; print('ok')"])
        .output();
    let python_ok = match py {
        Ok(o) => o.status.success(),
        Err(_) => false,
    };
    if !python_ok {
        return Err(crate::error::llama_cpp_missing());
    }
    let status = std::process::Command::new("python3")
        .args(["-m", "llama_cpp.quantize", path, &out, target])
        .status()?;
    if !status.success() {
        return Err(crate::error::llama_quantize_failed(status.code()));
    }
    println!("quantized: {} ({target})", out);
    println!("push to registry: mgc model push {out} --repo ai/<name> (compressed variant)");
    Ok(())
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
    let data = tokio::fs::read(path).await?;
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&data);
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
