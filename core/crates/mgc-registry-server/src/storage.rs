//! Storage backend for registry server
//! (Storage: SQLite for metadata, filesystem for blobs)

use anyhow::{Context, Result};
use base64::Engine;
use serde_json;
use sqlx::{Pool, Row, Sqlite, SqlitePool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DEFAULT_MAX_OCI_BLOB_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const DEFAULT_MAX_OCI_MANIFEST_VERIFY_BYTES: u64 = 40 * 1024 * 1024 * 1024;

pub(crate) fn max_oci_blob_bytes() -> u64 {
    std::env::var("MGC_REGISTRY_MAX_OCI_BLOB_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(DEFAULT_MAX_OCI_BLOB_BYTES)
}

pub(crate) fn max_oci_manifest_verify_bytes() -> u64 {
    std::env::var("MGC_REGISTRY_MAX_OCI_MANIFEST_VERIFY_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|limit| *limit > 0)
        .unwrap_or(DEFAULT_MAX_OCI_MANIFEST_VERIFY_BYTES)
}

fn validate_oci_blob_size(size: u64, limit: u64) -> Result<(), OciUploadAppendError> {
    if size > limit {
        Err(OciUploadAppendError::TooLarge)
    } else {
        Ok(())
    }
}

/// Hex fragment của digest để làm path component an toàn.
/// - "sha512-<b64>": decode base64 → hex
/// - chuỗi hex thuần (vd "sha256:<hex>", hoặc hex không prefix): dùng trực tiếp
///
/// (Không slice raw b64 — b64 có thể bắt đầu `/`/`+` khiến Path::join tạo path
/// tuyệt đối, vd join("/+") → "/+/..." → tạo thư mục ở root → 500.)
///
/// Fail-closed: malformed digest (empty body, bad hex/b64) trả Err — không
/// bao giờ decode lỗi thành empty rồi lưu path rác.
fn digest_hex_path(digest: &str) -> Result<(String, String)> {
    const MIN_DIGEST_HEX: usize = 8;

    let (algo, body) = match digest.rfind(['-', ':']) {
        Some(i) => (&digest[..i], &digest[i + 1..]),
        None => ("", digest),
    };
    if body.is_empty() {
        anyhow::bail!("malformed digest '{digest}': empty body");
    }
    let bytes = if body.bytes().all(|c| c.is_ascii_hexdigit()) {
        if body.len() % 2 != 0 {
            anyhow::bail!("malformed digest '{digest}': odd-length hex body");
        }
        (0..body.len() / 2)
            .map(|i| {
                u8::from_str_radix(&body[i * 2..i * 2 + 2], 16)
                    .map_err(|_| anyhow::anyhow!("malformed digest '{digest}': bad hex pair"))
            })
            .collect::<Result<Vec<u8>>>()?
    } else {
        let algo_prefix = algo.split(':').next().unwrap_or("sha512");
        if !algo_prefix.contains("sha") {
            anyhow::bail!("malformed digest '{digest}': unknown algorithm '{algo_prefix}'");
        }
        base64::engine::general_purpose::STANDARD
            .decode(body)
            .map_err(|_| anyhow::anyhow!("malformed digest '{digest}': invalid base64 body"))?
    };
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if hex.len() < MIN_DIGEST_HEX {
        anyhow::bail!("malformed digest '{digest}': shorter than {MIN_DIGEST_HEX} hex chars");
    }
    Ok((hex[..2].to_string(), hex[2..].to_string()))
}

/// Validate a repo/uuid/path segment before it reaches the filesystem.
/// OCI repo names may contain `/` — each component is validated separately.
/// Only [A-Za-z0-9._-] allowed per component, no dots-only (".", ".."),
/// no separators, no backslash.
///
/// Validate segment repo/uuid/path trước khi chạm filesystem. Tên repo OCI
/// có thể chứa `/` — validate từng component: chỉ cho phép [A-Za-z0-9._-],
/// cấm toàn dấu chấm, ký tự phân tách và backslash.
fn validate_fs_segment(segment: &str) -> Result<()> {
    const MAX_SEGMENT_LEN: usize = 255;
    let bad = |s: &str| {
        s.is_empty()
            || s.len() > MAX_SEGMENT_LEN
            || s == "."
            || s == ".."
            || s.contains('\\')
            || s.contains('/')
            || !s
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'_' || c == b'-')
    };
    // A whole-segment slash is legal only for repos (OCI path names); split
    // and check every component. UUIDs and single segments have no '/'.
    // Dấu `/` chỉ hợp pháp trong repo (tên OCI path) — tách và check từng
    // component. UUID và segment đơn không chứa '/'.
    if segment.contains('/') {
        for component in segment.split('/') {
            if bad(component) {
                anyhow::bail!("invalid filesystem segment: '{segment}' (component '{component}')");
            }
        }
        return Ok(());
    }
    if bad(segment) {
        anyhow::bail!("invalid filesystem segment: '{segment}'");
    }
    Ok(())
}

/// Registry storage backend
type OciUploadLockMap =
    std::sync::Mutex<HashMap<(String, String), std::sync::Weak<tokio::sync::Mutex<()>>>>;

pub struct RegistryStore {
    db: Pool<Sqlite>,
    blobs_dir: PathBuf,
    upstream: Option<Upstream>,
    backend: BlobBackend,
    upload_locks: OciUploadLockMap,
}

#[derive(Debug)]
pub enum OciUploadAppendError {
    SessionNotFound,
    OffsetMismatch,
    TooLarge,
    Storage(anyhow::Error),
}

impl From<anyhow::Error> for OciUploadAppendError {
    fn from(error: anyhow::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<sqlx::Error> for OciUploadAppendError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error.into())
    }
}

impl From<std::io::Error> for OciUploadAppendError {
    fn from(error: std::io::Error) -> Self {
        Self::Storage(error.into())
    }
}

impl From<std::num::TryFromIntError> for OciUploadAppendError {
    fn from(error: std::num::TryFromIntError) -> Self {
        Self::Storage(error.into())
    }
}

/// Blob storage backend (ITEM 5): Local FS hoặc S3-compatible (object_store).
pub enum BlobBackend {
    Local(PathBuf),
    S3(std::sync::Arc<object_store::aws::AmazonS3>),
}

use object_store::ObjectStoreExt as _;

/// Typed blob presence (audit vòng-3 P1-6): existence alone cannot be
/// trusted as "cache hit" — a torn file still occupies the final path.
/// Callers branch on the honest state instead of a boolean.
///
/// Trạng thái blob typed (P1-6 audit vòng-3): chỉ tồn tại không thể tin là
/// "cache hit" — file đứt vẫn chiếm path cuối. Caller phân nhánh theo trạng
/// thái trung thực thay vì boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobPresence {
    /// No DB row, or the backing file is gone.
    /// (Không có row DB, hoặc file nền đã mất.)
    Missing,
    /// File exists and its LENGTH matches the DB record — length only, NOT
    /// a digest verification (renamed from `PresentVerified` per audit
    /// vòng-4 P1: the old name overclaimed; two different payloads with the
    /// same length satisfied it). The digest is fully verified at serve
    /// time in get_blob.
    /// (File tồn tại và ĐỘ DÀI khớp bản ghi DB — chỉ độ dài, KHÔNG phải
    /// verify digest (đổi tên từ `PresentVerified` theo P1 audit vòng-4:
    /// tên cũ claim quá mức; 2 payload khác nhau cùng độ dài vẫn thỏa).
    /// Digest được verify đầy đủ lúc serve trong get_blob.)
    PresentLengthMatched,
    /// File exists but its length disagrees with the DB — treat as
    /// "needs repair", never as a cache hit.
    /// (File có nhưng độ dài lệch DB — coi là "cần sửa", không phải cache
    /// hit.)
    PresentLengthMismatch,
}

/// RAII cleanup for atomic-write temp files — removes the temp on drop
/// unless disarmed after a successful rename (P0-5).
/// Dọn temp của ghi nguyên tử bằng RAII — xóa temp khi drop trừ khi disarm
/// sau khi rename thành công (P0-5).
struct TempGuard<'a> {
    path: &'a Path,
    armed: bool,
}

impl<'a> TempGuard<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, armed: true }
    }

    /// Disarm after the temp has been renamed away (nothing to clean).
    /// Disarm sau khi temp đã được rename đi (không còn gì để dọn).
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            // Best-effort: sync removal of our own unique temp.
            // Best-effort: xóa sync temp duy nhất của mình.
            let _ = std::fs::remove_file(self.path);
        }
    }
}

/// Unique temp path inside `parent` for atomic local writes.
/// Đường dẫn temp duy nhất trong `parent` cho ghi local nguyên tử.
fn parent_temp_path(parent: &Path) -> PathBuf {
    parent.join(format!(
        ".put-{}-{}",
        uuid::Uuid::new_v4(),
        std::process::id()
    ))
}

impl BlobBackend {
    /// "local" hoặc "s3://bucket/prefix"
    pub fn parse(spec: Option<&str>, local_dir: &Path) -> Result<Self> {
        match spec {
            None | Some("local") => Ok(BlobBackend::Local(local_dir.to_path_buf())),
            Some(s3) => {
                let store = object_store::aws::AmazonS3Builder::from_env()
                    .with_url(s3)
                    .build()
                    .context("build S3 backend from env (AWS_ACCESS_KEY_ID/SECRET/REGION)")?;
                Ok(BlobBackend::S3(std::sync::Arc::new(store)))
            }
        }
    }

    /// Atomic local write (P0-5): unique temp in the SAME directory → write
    /// → flush → fsync → rename. A crash mid-write can never leave a torn
    /// blob at the final path; the temp is cleaned up on every failure path
    /// (TempGuard RAII). S3 puts are already atomic at the object level.
    ///
    /// Ghi local nguyên tử (P0-5): temp duy nhất CÙNG thư mục → ghi → flush
    /// → fsync → rename. Crash giữa chừng không thể để blob cụt ở path cuối;
    /// temp được dọn ở mọi đường lỗi (RAII TempGuard). Put S3 vốn đã nguyên
    /// tử ở cấp object.
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        match self {
            BlobBackend::Local(dir) => {
                let path = dir.join(key);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).await?;
                }

                // Unique temp next to the final path (same filesystem →
                // rename is atomic; Windows rename-over-existing is avoided
                // by renaming only when the final path is absent, and a
                // failed rename falls through to verified-reuse below).
                // Temp duy nhất cạnh path cuối (cùng filesystem → rename
                // nguyên tử; tránh rename-đè-trên-Windows bằng cách chỉ
                // rename khi path cuối chưa có, lỗi rename thì rơi xuống
                // đường verified-reuse phía dưới).
                let tmp_parent = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| dir.clone());
                let tmp = parent_temp_path(&tmp_parent);
                let mut guard = TempGuard::new(&tmp);

                let mut file = fs::File::create(&tmp).await?;
                file.write_all(data).await?;
                file.flush().await?;
                let sync = file.sync_all().await;
                drop(file);
                sync?;

                match fs::rename(&tmp, &path).await {
                    Ok(()) => {
                        guard.disarm();
                        Ok(())
                    }
                    Err(e) if path.exists() => {
                        // Lost a concurrent put of the same key — the winner
                        // already published; our temp is dropped by the guard.
                        // Thua put song song cùng key — winner đã publish; temp
                        // của ta được guard dọn.
                        let _ = e;
                        Ok(())
                    }
                    Err(e) => Err(anyhow::anyhow!("atomic blob put failed for '{key}': {e}")),
                }
            }
            BlobBackend::S3(store) => {
                let _ = store
                    .put(
                        &object_store::path::Path::from(key.to_string()),
                        object_store::PutPayload::from(data.to_vec()),
                    )
                    .await
                    .context("S3 put")?;
                Ok(())
            }
        }
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match self {
            BlobBackend::Local(dir) => {
                let path = dir.join(key);
                match fs::read(&path).await {
                    Ok(data) => Ok(Some(data)),
                    Err(_) => Ok(None),
                }
            }
            BlobBackend::S3(store) => {
                let resp = store
                    .get(&object_store::path::Path::from(key.to_string()))
                    .await;
                match resp {
                    Ok(r) => Ok(Some(r.bytes().await.context("S3 get bytes")?.to_vec())),
                    Err(object_store::Error::NotFound { .. }) => Ok(None),
                    Err(e) => Err(anyhow::anyhow!(e)),
                }
            }
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        match self {
            BlobBackend::Local(dir) => Ok(dir.join(key).exists()),
            BlobBackend::S3(store) => {
                match store
                    .head(&object_store::path::Path::from(key.to_string()))
                    .await
                {
                    Ok(_) => Ok(true),
                    Err(object_store::Error::NotFound { .. }) => Ok(false),
                    Err(e) => Err(anyhow::anyhow!(e)),
                }
            }
        }
    }

    /// Published blob length (getter used by put-verify and blob_exists).
    /// Returns Err when the key is missing (caller checks exists() first
    /// when existence is the question).
    ///
    /// Độ dài blob đã publish (getter cho put-verify và blob_exists). Trả
    /// Err khi key mất (caller check exists() trước nếu câu hỏi là tồn tại.)
    async fn published_len(&self, key: &str) -> Result<u64> {
        match self {
            BlobBackend::Local(dir) => {
                let meta = fs::metadata(dir.join(key)).await?;
                Ok(meta.len())
            }
            BlobBackend::S3(store) => {
                let head = store
                    .head(&object_store::path::Path::from(key.to_string()))
                    .await
                    .context("S3 head for published length")?;
                Ok(head.size)
            }
        }
    }

    /// Verify the published blob's length matches what we intended to store
    /// (vòng-2 review R-put): catches a pre-existing torn/stale file at the
    /// final path that `put` legitimately skipped overwriting.
    ///
    /// Verify độ dài blob đã publish khớp với bytes định lưu (R-put vòng-2):
    /// bắt file cụt/cũ đã tồn tại sẵn ở path cuối mà `put` đã hợp lệ bỏ qua
    /// ghi đè.
    async fn verify_published_len(&self, key: &str, expected_len: usize) -> Result<()> {
        let actual = self.published_len(key).await?;
        if actual != expected_len as u64 {
            anyhow::bail!(
                "published blob length mismatch for '{key}': expected {expected_len} bytes, found {actual} (torn or stale blob at final path)"
            );
        }
        Ok(())
    }
}

/// Upstream proxy (ITEM 4): GET miss → fetch từ registry upstream → cache local.
pub struct Upstream {
    base: String,
    client: reqwest::Client,
}

impl Upstream {
    /// Request timeout for upstream fetches (registry metadata + tarballs).
    /// Timeout cho request upstream (metadata + tarball).
    const REQUEST_TIMEOUT_SECS: u64 = 30;

    /// Hard cap on a single upstream response body (64 MiB) — a hostile or
    /// misbehaving upstream cannot balloon registry memory.
    /// Giới hạn cứng body 1 response upstream (64 MiB) — upstream xấu không
    /// thể phình memory của registry.
    const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

    pub fn new(base: String) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .user_agent("magicore-registry/0.1")
                .timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
                .connect_timeout(std::time::Duration::from_secs(Self::REQUEST_TIMEOUT_SECS))
                // No redirects: the SSRF same-host check validates the FIRST
                // URL only; a redirect could bounce to an attacker host.
                // Không redirect: check cùng-host SSRF chỉ áp URL đầu tiên;
                // redirect có thể nhảy sang host của kẻ tấn công.
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default(),
        }
    }

    async fn fetch_json(&self, name: &str) -> Result<Option<serde_json::Value>> {
        let url = format!("{}/{}", self.base, name);
        let resp = self.client.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Ok(None);
        }
        let body = resp.bytes().await?;
        if body.len() > Self::MAX_RESPONSE_BYTES {
            anyhow::bail!("upstream response exceeds size limit");
        }
        Ok(Some(serde_json::from_slice(&body)?))
    }

    async fn fetch_bytes(&self, url: &str) -> Result<Option<Vec<u8>>> {
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let body = resp.bytes().await?;
        if body.len() > Self::MAX_RESPONSE_BYTES {
            anyhow::bail!("upstream response exceeds size limit");
        }
        Ok(Some(body.to_vec()))
    }
}

impl RegistryStore {
    /// Create new registry store
    pub async fn new<P: AsRef<Path>>(store_dir: P) -> Result<Self> {
        let store_dir = store_dir.as_ref().to_path_buf();
        let blobs_dir = store_dir.join("blobs");
        let db_path = store_dir.join("registry.db");

        fs::create_dir_all(&store_dir).await?;
        fs::create_dir_all(&blobs_dir).await?;

        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePool::connect(&db_url)
            .await
            .context("Failed to connect to SQLite")?;

        // Enable Crash-Atomic WAL mode and performance optimizations
        sqlx::query("PRAGMA journal_mode = WAL;")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("PRAGMA synchronous = NORMAL;")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("PRAGMA busy_timeout = 5000;")
            .execute(&pool)
            .await
            .ok();

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("Failed to run migrations")?;

        let store = Self {
            db: pool,
            blobs_dir: blobs_dir.clone(),
            upstream: None,
            backend: BlobBackend::Local(blobs_dir),
            upload_locks: std::sync::Mutex::new(HashMap::new()),
        };

        store.init_schema().await?;
        store.migrate_pypi_project_names().await?;

        Ok(store)
    }

    async fn migrate_pypi_project_names(&self) -> Result<()> {
        const MIGRATION: &str = "canonical-pypi-project-names-v1";
        let mut tx = self.db.begin().await?;
        let already_applied = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM mgc_runtime_migrations WHERE name = ?)",
        )
        .bind(MIGRATION)
        .fetch_one(&mut *tx)
        .await?;
        if already_applied != 0 {
            tx.rollback().await?;
            return Ok(());
        }

        let rows = sqlx::query(
            "SELECT name, version, filename, digest, size, requires_python, created_at FROM pypi_files ORDER BY rowid",
        )
        .fetch_all(&mut *tx)
        .await?;
        let mut files: std::collections::BTreeMap<
            (String, String),
            (crate::model::PypiFile, String),
        > = std::collections::BTreeMap::new();
        for row in rows {
            let original_name = row.get::<String, _>("name");
            let name = crate::trusted::canonical_pypi_name(&original_name)
                .unwrap_or_else(|_| original_name.clone());
            let file = crate::model::PypiFile {
                name: name.clone(),
                version: row.get("version"),
                filename: row.get("filename"),
                digest: row.get("digest"),
                size: row.get("size"),
                requires_python: row.get("requires_python"),
            };
            let key = (name, file.filename.clone());
            let created_at: String = row.get("created_at");
            if let Some((existing, _)) = files.get(&key) {
                if existing.version != file.version
                    || existing.digest != file.digest
                    || existing.size != file.size
                    || existing.requires_python != file.requires_python
                {
                    anyhow::bail!(
                        "legacy PyPI aliases contain conflicting file {}; resolve the duplicate before starting the registry",
                        file.filename
                    );
                }
            } else {
                files.insert(key, (file, created_at));
            }
        }

        sqlx::query("DELETE FROM pypi_files")
            .execute(&mut *tx)
            .await?;
        for (_, (file, created_at)) in files {
            sqlx::query(
                "INSERT INTO pypi_files (name, version, filename, digest, size, requires_python, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&file.name)
            .bind(&file.version)
            .bind(&file.filename)
            .bind(&file.digest)
            .bind(file.size)
            .bind(&file.requires_python)
            .bind(created_at)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("INSERT INTO mgc_runtime_migrations (name) VALUES (?)")
            .bind(MIGRATION)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn init_schema(&self) -> Result<()> {
        // Packages table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS packages (
                name TEXT PRIMARY KEY,
                description TEXT,
                dist_tags TEXT NOT NULL DEFAULT '{}',
                maintainers TEXT NOT NULL DEFAULT '[]',
                time TEXT NOT NULL,
                private BOOLEAN NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&self.db)
        .await?;

        // Package versions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS package_versions (
                id TEXT PRIMARY KEY,
                package_name TEXT NOT NULL,
                version TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.db)
        .await?;

        // Blobs table (for OCI)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS blobs (
                digest TEXT PRIMARY KEY,
                size INTEGER NOT NULL,
                path TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&self.db)
        .await?;

        // OCI manifests table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS oci_manifests (
                repo TEXT NOT NULL,
                reference TEXT NOT NULL,
                manifest TEXT NOT NULL,
                digest TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (repo, reference)
            )
        "#,
        )
        .execute(&self.db)
        .await?;

        // OCI blobs table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS oci_blobs (
                repo TEXT NOT NULL,
                digest TEXT NOT NULL,
                size INTEGER NOT NULL,
                path TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (repo, digest)
            )
        "#,
        )
        .execute(&self.db)
        .await?;

        // OCI upload sessions (chunked/resumable)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS oci_uploads (
                repo TEXT NOT NULL,
                uuid TEXT NOT NULL,
                path TEXT NOT NULL,
                offset_bytes INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (repo, uuid)
            )
        "#,
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Audit log event (task #3: publish/delete/upload → SQLite)
    pub async fn audit(
        &self,
        event_type: &str,
        name: &str,
        version: Option<&str>,
        user: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO audit_log (event_type, name, version, user)
            VALUES (?, ?, ?, ?)
        "#,
        )
        .bind(event_type)
        .bind(name)
        .bind(version)
        .bind(user)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    // === Package operations ===

    /// Cấu hình upstream proxy (ITEM 4). None = registry đóng (private-only).
    pub fn set_upstream(&mut self, upstream: Option<String>) {
        self.upstream = upstream.map(Upstream::new);
    }

    /// Cấu hình blob backend (ITEM 5): "local" hoặc "s3://bucket/prefix".
    pub fn set_backend(&mut self, spec: Option<&str>) -> Result<()> {
        self.backend = BlobBackend::parse(spec, &self.blobs_dir)?;
        Ok(())
    }

    pub async fn get_package(&self, name: &str) -> Result<Option<crate::model::Package>> {
        let row = sqlx::query(
            r#"
            SELECT name, description, dist_tags, maintainers, time, private
            FROM packages WHERE name = ?
        "#,
        )
        .bind(name)
        .fetch_optional(&self.db)
        .await?;

        if let Some(row) = row {
            let mut pkg = crate::model::Package {
                name: row.get("name"),
                description: row.get("description"),
                versions: std::collections::HashMap::new(),
                dist_tags: serde_json::from_str(&row.get::<String, _>("dist_tags"))?,
                maintainers: serde_json::from_str(&row.get::<String, _>("maintainers"))?,
                time: serde_json::from_str(&row.get::<String, _>("time"))?,
                private: row.get("private"),
            };
            // Load versions
            let versions = self.get_package_versions(name).await?;
            pkg.versions = versions;
            Ok(Some(pkg))
        } else if let Some(upstream) = &self.upstream {
            // ITEM 4: miss → fetch upstream → cache local (private store giữ public mirror)
            match upstream.fetch_json(name).await {
                Ok(Some(json)) => match serde_json::from_value::<crate::model::Package>(json) {
                    Ok(pkg) => {
                        let _ = self.put_package(&pkg).await;
                        Ok(Some(pkg))
                    }
                    Err(_) => Ok(None),
                },
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    pub async fn get_package_versions(
        &self,
        name: &str,
    ) -> Result<std::collections::HashMap<String, crate::model::PackageVersion>> {
        let rows = sqlx::query(
            r#"
            SELECT version, data FROM package_versions WHERE package_name = ?
        "#,
        )
        .bind(name)
        .fetch_all(&self.db)
        .await?;

        let mut versions = std::collections::HashMap::new();
        for row in rows {
            let version: String = row.get("version");
            let data: String = row.get("data");
            let ver: crate::model::PackageVersion = serde_json::from_str(&data)?;
            versions.insert(version, ver);
        }
        Ok(versions)
    }

    pub async fn put_package(&self, pkg: &crate::model::Package) -> Result<()> {
        let mut tx = self.db.begin().await?;
        let binding = sqlx::query_scalar::<_, String>(
            "SELECT repository FROM trusted_publishers WHERE package = ?",
        )
        .bind(&pkg.name)
        .fetch_optional(&mut *tx)
        .await?;
        if binding.is_some() {
            anyhow::bail!("package requires trusted OIDC publishing");
        }
        let time_json = serde_json::to_string(&pkg.time)?;
        let dist_tags_json = serde_json::to_string(&pkg.dist_tags)?;
        let maintainers_json = serde_json::to_string(&pkg.maintainers)?;

        sqlx::query(
            r#"
            INSERT INTO packages (name, description, dist_tags, maintainers, time, private)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                description = excluded.description,
                dist_tags = excluded.dist_tags,
                maintainers = excluded.maintainers,
                time = excluded.time,
                private = excluded.private,
                updated_at = datetime('now')
        "#,
        )
        .bind(&pkg.name)
        .bind(&pkg.description)
        .bind(&dist_tags_json)
        .bind(&maintainers_json)
        .bind(&time_json)
        .bind(pkg.private)
        .execute(&mut *tx)
        .await?;

        // Save versions
        for (version, ver) in &pkg.versions {
            let _ver_json = serde_json::to_string(ver)?;
            let id = format!("{}@{}", pkg.name, version);
            sqlx::query(
                r#"
                INSERT INTO package_versions (id, package_name, version, data)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET data = excluded.data
            "#,
            )
            .bind(&id)
            .bind(&pkg.name)
            .bind(version)
            .bind(serde_json::to_string(ver)?)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Store a package and its signed provenance entries in one SQLite transaction.
    /// Lưu package cùng provenance đã ký trong một transaction SQLite.
    pub async fn put_trusted_package(
        &self,
        pkg: &crate::model::Package,
        claims: &mgc_oidc::OidcClaims,
        key: &mgc_crypto::KeyPair,
        expected_binding_generation: u64,
        public_attestations: &HashMap<String, (String, serde_json::Value)>,
    ) -> Result<()> {
        crate::trusted::scoped_package("npm", &pkg.name)
            .map_err(|_| anyhow::anyhow!("trusted npm package name is invalid"))?;
        if pkg.versions.is_empty() {
            anyhow::bail!("trusted publish must include at least one version");
        }

        let mut tx = self.db.begin().await?;
        let binding = sqlx::query(
            "SELECT repository, binding_generation FROM trusted_publishers WHERE package = ?",
        )
        .bind(&pkg.name)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(repository) = binding else {
            anyhow::bail!("trusted publish requires an active package binding");
        };
        let binding_generation = u64::try_from(repository.get::<i64, _>("binding_generation"))
            .context("trusted package binding generation is invalid")?;
        if binding_generation != expected_binding_generation {
            anyhow::bail!("trusted package binding changed after token authorization");
        }
        let repository = repository.get::<String, _>("repository");
        if !crate::trusted::claims_match_repository_binding(claims, &repository) {
            anyhow::bail!("trusted publisher claims do not match the active package binding");
        }
        let last = sqlx::query(
            "SELECT sequence, entry_hash FROM trusted_attestations WHERE package = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(&pkg.name)
        .fetch_optional(&mut *tx)
        .await?;
        let sequence = last
            .as_ref()
            .map(|row| row.get::<i64, _>("sequence"))
            .unwrap_or(0);
        if sequence < 0 {
            anyhow::bail!("trusted attestation sequence is invalid");
        }
        let mut sequence = sequence as u64;
        let mut previous_hash = last.map(|row| row.get::<String, _>("entry_hash"));

        let mut dist_tags =
            sqlx::query_scalar::<_, String>("SELECT dist_tags FROM packages WHERE name = ?")
                .bind(&pkg.name)
                .fetch_optional(&mut *tx)
                .await?
                .map(|stored| serde_json::from_str::<HashMap<String, String>>(&stored))
                .transpose()?
                .unwrap_or_default();
        dist_tags.extend(pkg.dist_tags.clone());
        let dist_tags_json = serde_json::to_string(&dist_tags)?;
        let time_json = serde_json::to_string(&pkg.time)?;
        let maintainers_json = serde_json::to_string(&pkg.maintainers)?;
        sqlx::query(
            "INSERT INTO packages (name, description, dist_tags, maintainers, time, private) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(name) DO UPDATE SET description = excluded.description, dist_tags = excluded.dist_tags, maintainers = excluded.maintainers, time = excluded.time, private = excluded.private, updated_at = datetime('now')",
        )
        .bind(&pkg.name)
        .bind(&pkg.description)
        .bind(&dist_tags_json)
        .bind(&maintainers_json)
        .bind(&time_json)
        .bind(pkg.private)
        .execute(&mut *tx)
        .await?;

        let mut versions = pkg.versions.iter().collect::<Vec<_>>();
        versions.sort_by(|left, right| left.0.cmp(right.0));
        for (version, package_version) in versions {
            let digest = package_version.dist.integrity.trim();
            if !crate::trusted::is_valid_sha512_integrity(digest) {
                anyhow::bail!("trusted publish requires a valid npm SHA-512 integrity digest");
            }
            let id = format!("{}@{}", pkg.name, version);
            sqlx::query(
                "INSERT INTO package_versions (id, package_name, version, data) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            )
            .bind(&id)
            .bind(&pkg.name)
            .bind(version)
            .bind(serde_json::to_string(package_version)?)
            .execute(&mut *tx)
            .await?;

            sequence = sequence
                .checked_add(1)
                .filter(|next| *next <= i64::MAX as u64)
                .ok_or_else(|| anyhow::anyhow!("trusted attestation sequence is exhausted"))?;
            let (artifact_sha256, sigstore_bundle) =
                public_attestations.get(version).ok_or_else(|| {
                    anyhow::anyhow!("trusted publish is missing its public Sigstore attestation")
                })?;
            if !crate::trusted::is_valid_sha256_hex(artifact_sha256) {
                anyhow::bail!("trusted publish requires a valid artifact SHA-256 digest");
            }
            let payload = crate::trusted::AttestationPayload {
                schema_version: 1,
                package: pkg.name.clone(),
                version: version.clone(),
                digest: digest.to_string(),
                artifact_sha256: Some(artifact_sha256.clone()),
                sequence,
                builder: crate::trusted::BuilderIdentity::from(claims),
                created_at_unix: chrono::Utc::now().timestamp().max(0) as u64,
                previous_hash: previous_hash.clone(),
            };
            let mut entry = crate::trusted::sign_attestation(payload, key)?;
            entry.sigstore_bundle = Some(sigstore_bundle.clone());
            sqlx::query(
                "INSERT INTO trusted_attestations (package, sequence, payload, entry_hash, key_id, signature, sigstore_bundle) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&pkg.name)
            .bind(sequence as i64)
            .bind(serde_json::to_string(&entry.payload)?)
            .bind(&entry.entry_hash)
            .bind(&entry.key_id)
            .bind(&entry.signature)
            .bind(serde_json::to_string(sigstore_bundle)?)
            .execute(&mut *tx)
            .await?;
            previous_hash = Some(entry.entry_hash);
        }
        tx.commit().await?;
        Ok(())
    }

    /// Append one signed record for a PyPI distribution or OCI manifest.
    /// Ghi một provenance đã ký cho distribution PyPI hoặc manifest OCI.
    #[allow(clippy::too_many_arguments)]
    pub async fn append_trusted_attestation(
        &self,
        package: &str,
        version: &str,
        registry_digest: &str,
        artifact_sha256: &str,
        claims: &mgc_oidc::OidcClaims,
        key: &mgc_crypto::KeyPair,
        expected_binding_generation: u64,
        sigstore_bundle: serde_json::Value,
    ) -> Result<()> {
        let mut tx = self.db.begin().await?;
        Self::append_trusted_attestation_tx(
            &mut tx,
            package,
            version,
            registry_digest,
            artifact_sha256,
            claims,
            key,
            expected_binding_generation,
            sigstore_bundle,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Store a trusted PyPI file and its Sigstore record atomically.
    /// Lưu file PyPI trusted cùng bản ghi Sigstore trong cùng transaction.
    pub async fn put_trusted_pypi_file(
        &self,
        file: &crate::model::PypiFile,
        claims: &mgc_oidc::OidcClaims,
        key: &mgc_crypto::KeyPair,
        expected_binding_generation: u64,
        artifact_sha256: &str,
        sigstore_bundle: serde_json::Value,
    ) -> Result<()> {
        let mut canonical_file = file.clone();
        canonical_file.name = crate::trusted::canonical_pypi_name(&file.name)
            .map_err(|_| anyhow::anyhow!("invalid PyPI project name"))?;
        let package = crate::trusted::scoped_package("pypi", &canonical_file.name)
            .map_err(|_| anyhow::anyhow!("invalid PyPI project name"))?;
        let mut tx = self.db.begin().await?;
        sqlx::query(
            r#"INSERT INTO pypi_files (name, version, filename, digest, size, requires_python)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT(name, filename) DO UPDATE SET version = excluded.version,
               digest = excluded.digest, size = excluded.size, requires_python = excluded.requires_python"#,
        )
        .bind(&canonical_file.name)
        .bind(&canonical_file.version)
        .bind(&canonical_file.filename)
        .bind(&canonical_file.digest)
        .bind(canonical_file.size)
        .bind(&canonical_file.requires_python)
        .execute(&mut *tx)
        .await?;
        Self::append_trusted_attestation_tx(
            &mut tx,
            &package,
            &file.version,
            &file.digest,
            artifact_sha256,
            claims,
            key,
            expected_binding_generation,
            sigstore_bundle,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Store a trusted OCI manifest and its Sigstore record atomically.
    /// Lưu manifest OCI trusted cùng bản ghi Sigstore trong cùng transaction.
    #[allow(clippy::too_many_arguments)]
    pub async fn put_trusted_oci_manifest(
        &self,
        package_scope: &str,
        repo: &str,
        reference: &str,
        manifest_bytes: &[u8],
        registry_digest: &str,
        artifact_sha256: &str,
        claims: &mgc_oidc::OidcClaims,
        key: &mgc_crypto::KeyPair,
        expected_binding_generation: u64,
        sigstore_bundle: serde_json::Value,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        let actual_sha256 = hex::encode(Sha256::digest(manifest_bytes));
        if actual_sha256 != artifact_sha256 || registry_digest != format!("sha256:{actual_sha256}")
        {
            anyhow::bail!("trusted OCI manifest digest does not match its bytes");
        }
        let manifest_json = String::from_utf8(manifest_bytes.to_vec())
            .context("OCI manifest is not valid UTF-8")?;
        let mut tx = self.db.begin().await?;
        sqlx::query(
            r#"INSERT INTO oci_manifests (repo, reference, manifest, digest)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(repo, reference) DO UPDATE SET manifest = excluded.manifest,
               digest = excluded.digest"#,
        )
        .bind(repo)
        .bind(reference)
        .bind(manifest_json)
        .bind(registry_digest)
        .execute(&mut *tx)
        .await?;
        Self::append_trusted_attestation_tx(
            &mut tx,
            package_scope,
            reference,
            registry_digest,
            artifact_sha256,
            claims,
            key,
            expected_binding_generation,
            sigstore_bundle,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn append_trusted_attestation_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        package: &str,
        version: &str,
        registry_digest: &str,
        artifact_sha256: &str,
        claims: &mgc_oidc::OidcClaims,
        key: &mgc_crypto::KeyPair,
        expected_binding_generation: u64,
        sigstore_bundle: serde_json::Value,
    ) -> Result<()> {
        if !crate::trusted::is_valid_sha256_hex(artifact_sha256) {
            anyhow::bail!("trusted publish requires a valid artifact SHA-256 digest");
        }
        let binding = sqlx::query(
            "SELECT repository, binding_generation FROM trusted_publishers WHERE package = ?",
        )
        .bind(package)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(binding) = binding else {
            anyhow::bail!("trusted publish requires an active package binding");
        };
        let generation = u64::try_from(binding.get::<i64, _>("binding_generation"))
            .context("trusted package binding generation is invalid")?;
        if generation != expected_binding_generation {
            anyhow::bail!("trusted package binding changed after token authorization");
        }
        let repository = binding.get::<String, _>("repository");
        if !crate::trusted::claims_match_repository_binding(claims, &repository) {
            anyhow::bail!("trusted publisher claims do not match the active package binding");
        }
        let last = sqlx::query(
            "SELECT sequence, entry_hash FROM trusted_attestations WHERE package = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(package)
        .fetch_optional(&mut **tx)
        .await?;
        let previous_sequence = last
            .as_ref()
            .map(|row| row.get::<i64, _>("sequence"))
            .unwrap_or(0);
        if previous_sequence < 0 {
            anyhow::bail!("trusted attestation sequence is invalid");
        }
        let sequence = (previous_sequence as u64)
            .checked_add(1)
            .filter(|next| *next <= i64::MAX as u64)
            .ok_or_else(|| anyhow::anyhow!("trusted attestation sequence is exhausted"))?;
        let previous_hash = last.map(|row| row.get::<String, _>("entry_hash"));
        let payload = crate::trusted::AttestationPayload {
            schema_version: 1,
            package: package.to_owned(),
            version: version.to_owned(),
            digest: registry_digest.to_owned(),
            artifact_sha256: Some(artifact_sha256.to_owned()),
            sequence,
            builder: crate::trusted::BuilderIdentity::from(claims),
            created_at_unix: chrono::Utc::now().timestamp().max(0) as u64,
            previous_hash,
        };
        let mut entry = crate::trusted::sign_attestation(payload, key)?;
        entry.sigstore_bundle = Some(sigstore_bundle.clone());
        sqlx::query(
            "INSERT INTO trusted_attestations (package, sequence, payload, entry_hash, key_id, signature, sigstore_bundle) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(package)
        .bind(sequence as i64)
        .bind(serde_json::to_string(&entry.payload)?)
        .bind(&entry.entry_hash)
        .bind(&entry.key_id)
        .bind(&entry.signature)
        .bind(serde_json::to_string(&sigstore_bundle)?)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn set_trusted_publisher(&self, package: &str, repository: &str) -> Result<()> {
        let mut tx = self.db.begin().await?;
        sqlx::query(
            "INSERT INTO trusted_publishers (package, repository, binding_generation) VALUES (?, ?, 1) ON CONFLICT(package) DO UPDATE SET repository = excluded.repository, binding_generation = trusted_publishers.binding_generation + CASE WHEN trusted_publishers.repository != excluded.repository THEN 1 ELSE 0 END, updated_at = datetime('now')",
        )
        .bind(package)
        .bind(repository)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_log (event_type, name, version, user) VALUES (?, ?, NULL, ?)",
        )
        .bind("trusted-publisher-bound")
        .bind(package)
        .bind(repository)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn trusted_publisher(&self, package: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT repository FROM trusted_publishers WHERE package = ?")
            .bind(package)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|row| row.get("repository")))
    }

    pub async fn trusted_publisher_binding(&self, package: &str) -> Result<Option<(String, u64)>> {
        let row = sqlx::query(
            "SELECT repository, binding_generation FROM trusted_publishers WHERE package = ?",
        )
        .bind(package)
        .fetch_optional(&self.db)
        .await?;
        row.map(|row| {
            let generation = u64::try_from(row.get::<i64, _>("binding_generation"))
                .context("trusted package binding generation is invalid")?;
            Ok((row.get("repository"), generation))
        })
        .transpose()
    }

    pub async fn set_trusted_dist_tag(
        &self,
        package: &str,
        tag: &str,
        version: &str,
        expected_binding_generation: u64,
    ) -> Result<bool> {
        let mut tx = self.db.begin().await?;
        let Some(binding) =
            sqlx::query("SELECT binding_generation FROM trusted_publishers WHERE package = ?")
                .bind(package)
                .fetch_optional(&mut *tx)
                .await?
        else {
            return Ok(false);
        };
        let binding_generation = u64::try_from(binding.get::<i64, _>("binding_generation"))
            .context("trusted package binding generation is invalid")?;
        if binding_generation != expected_binding_generation {
            return Ok(false);
        }
        let has_version: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM package_versions WHERE package_name = ? AND version = ?)",
        )
        .bind(package)
        .bind(version)
        .fetch_one(&mut *tx)
        .await?;
        if has_version == 0 {
            return Ok(false);
        }
        let Some(stored_tags) =
            sqlx::query_scalar::<_, String>("SELECT dist_tags FROM packages WHERE name = ?")
                .bind(package)
                .fetch_optional(&mut *tx)
                .await?
        else {
            return Ok(false);
        };
        let mut tags: HashMap<String, String> = serde_json::from_str(&stored_tags)?;
        tags.insert(tag.to_string(), version.to_string());
        sqlx::query(
            "UPDATE packages SET dist_tags = ?, updated_at = datetime('now') WHERE name = ?",
        )
        .bind(serde_json::to_string(&tags)?)
        .bind(package)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn has_trusted_attestations(&self) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM trusted_attestations)")
            .fetch_one(&self.db)
            .await?;
        Ok(exists != 0)
    }

    pub async fn trusted_attestation_key_id(&self) -> Result<Option<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT key_id FROM trusted_attestations LIMIT 2",
        )
        .fetch_all(&self.db)
        .await?;
        if rows.len() > 1 {
            anyhow::bail!("registry provenance contains records from unsupported signing keys");
        }
        Ok(rows.into_iter().next())
    }

    /// Read one immutable chain page, pinning the first page's head sequence.
    /// Đọc một trang chuỗi bất biến và giữ nguyên sequence head từ trang đầu.
    pub async fn trusted_attestations_page(
        &self,
        package: &str,
        after_sequence: u64,
        through_sequence: Option<u64>,
        limit: usize,
    ) -> Result<(u64, Vec<crate::trusted::SignedAttestation>)> {
        if limit == 0 || limit > crate::trusted::MAX_SIGNATURE_PAGE_SIZE {
            anyhow::bail!("trusted signature page size is outside the allowed range");
        }
        let current_head: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) FROM trusted_attestations WHERE package = ?",
        )
        .bind(package)
        .fetch_one(&self.db)
        .await?;
        let current_head =
            u64::try_from(current_head).context("trusted attestation head sequence is invalid")?;
        let head_sequence = through_sequence.unwrap_or(current_head).min(current_head);
        if after_sequence >= head_sequence {
            return Ok((head_sequence, Vec::new()));
        }
        let after_sequence = i64::try_from(after_sequence)
            .context("trusted attestation cursor is outside the supported range")?;
        let head_sequence = i64::try_from(head_sequence)
            .context("trusted attestation head is outside the supported range")?;
        let rows = sqlx::query(
            "SELECT payload, entry_hash, key_id, signature, sigstore_bundle FROM trusted_attestations WHERE package = ? AND sequence > ? AND sequence <= ? ORDER BY sequence ASC LIMIT ?",
        )
        .bind(package)
        .bind(after_sequence)
        .bind(head_sequence)
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;
        let entries = rows
            .into_iter()
            .map(|row| {
                Ok(crate::trusted::SignedAttestation {
                    payload: serde_json::from_str(&row.get::<String, _>("payload"))?,
                    entry_hash: row.get("entry_hash"),
                    key_id: row.get("key_id"),
                    signature: row.get("signature"),
                    sigstore_bundle: row
                        .get::<Option<String>, _>("sigstore_bundle")
                        .map(|value| serde_json::from_str(&value))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((head_sequence as u64, entries))
    }

    pub async fn trusted_attestations(
        &self,
        package: &str,
    ) -> Result<Vec<crate::trusted::SignedAttestation>> {
        let rows = sqlx::query(
            "SELECT payload, entry_hash, key_id, signature, sigstore_bundle FROM trusted_attestations WHERE package = ? ORDER BY sequence ASC",
        )
        .bind(package)
        .fetch_all(&self.db)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(crate::trusted::SignedAttestation {
                    payload: serde_json::from_str(&row.get::<String, _>("payload"))?,
                    entry_hash: row.get("entry_hash"),
                    key_id: row.get("key_id"),
                    signature: row.get("signature"),
                    sigstore_bundle: row
                        .get::<Option<String>, _>("sigstore_bundle")
                        .map(|value| serde_json::from_str(&value))
                        .transpose()?,
                })
            })
            .collect()
    }

    pub async fn delete_package(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM packages WHERE name = ?")
            .bind(name)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Xóa 1 version của package; trả true nếu version tồn tại
    /// (dist-tags trỏ tới version bị xóa cũng bị bỏ — npm behavior)
    pub async fn delete_package_version(&self, name: &str, version: &str) -> Result<bool> {
        let mut pkg = match self.get_package(name).await? {
            Some(p) => p,
            None => return Ok(false),
        };
        if pkg.versions.remove(version).is_none() {
            return Ok(false);
        }
        pkg.dist_tags.retain(|_, v| v != version);
        // Nếu hết version → xóa package luôn; ngược lại lưu lại
        if pkg.versions.is_empty() {
            self.delete_package(name).await?;
        } else {
            self.put_package(&pkg).await?;
        }
        Ok(true)
    }

    // === Blob operations ===

    /// Recompute the digest from the payload and refuse mismatches.
    /// Supports the two wire formats this registry serves:
    /// - "sha512-<base64>" (npm integrity style)
    /// - "sha256:<hex>" / "<hex>" (PyPI/OCI style)
    ///
    /// The stored digest is the SERVER-computed one; a client declaring
    /// digest A while uploading content B is rejected (P0-2).
    ///
    /// Tính lại digest từ payload và từ chối khi lệch. Server là nguồn chân lý
    /// của digest — client khai A nhưng upload B sẽ bị từ chối.
    fn compute_wire_digest(digest: &str, data: &[u8]) -> Result<String> {
        use sha2::{Digest, Sha256, Sha512};

        let (algo, _body) = match digest.rfind(['-', ':']) {
            Some(i) => (&digest[..i], &digest[i + 1..]),
            None => ("", digest),
        };

        if algo.contains("512") {
            let mut hasher = Sha512::new();
            hasher.update(data);
            let b64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                hasher.finalize(),
            );
            Ok(format!("sha512-{b64}"))
        } else {
            // sha256:<hex>, bare hex, or unknown algo — sha256 is the default
            // server-side truth (PyPI/OCI both use it).
            let mut hasher = Sha256::new();
            hasher.update(data);
            Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
        }
    }

    pub async fn put_blob(&self, digest: &str, data: &[u8]) -> Result<()> {
        let (p1, p2) = digest_hex_path(digest)?;
        let key = format!("{p1}/{p2}");

        // Server-side digest recomputation: the declared digest must match
        // the uploaded bytes exactly. A client declaring digest A while
        // uploading content B is rejected (P0-2) — provenance and
        // content-addressing stay trustworthy.
        // Server tính lại digest: digest khai phải khớp CHÍNH XÁC bytes
        // upload. Client khai A nhưng upload B bị từ chối (P0-2) — giữ
        // provenance và content-addressing đáng tin.
        let actual = Self::compute_wire_digest(digest, data)?;
        if actual != digest {
            anyhow::bail!("digest mismatch: declared '{digest}' but bytes hash to '{actual}'");
        }

        self.backend.put(&key, data).await?;

        // Vòng-2 review R-put: if the final path pre-existed (lost race, or
        // a torn leftover from an old crash), `put` may have skipped the
        // overwrite — verify the PUBLISHED file's length matches the bytes
        // we intended to store, so a torn/stale blob can never be recorded
        // in the DB as this digest.
        // (R-put vòng-2 review: nếu path cuối đã tồn tại trước (thua race,
        // hoặc rác cụt từ crash cũ), `put` có thể đã bỏ qua ghi đè — verify
        // độ dài file ĐÃ PUBLISH khớp bytes định lưu, để blob cụt/cũ không
        // bao giờ được DB ghi nhận là digest này.)
        self.backend.verify_published_len(&key, data.len()).await?;

        sqlx::query(
            r#"
            INSERT INTO blobs (digest, size, path)
            VALUES (?, ?, ?)
            ON CONFLICT(digest) DO UPDATE SET size = excluded.size, path = excluded.path
        "#,
        )
        .bind(digest)
        .bind(data.len() as i64)
        .bind(key)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn get_blob(&self, digest: &str) -> Result<Option<Vec<u8>>> {
        // Lookup by the canonical server-computed digest: same algorithm as
        // put_blob, so a declared digest resolves to the stored row.
        // Tra theo digest chuẩn do server tính: cùng thuật toán với put_blob
        // nên digest khai từ client vẫn trỏ đúng row đã lưu.
        let row = sqlx::query("SELECT path FROM blobs WHERE digest = ?")
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;

        if let Some(row) = row {
            let key: String = row.get("path");
            let data = self.backend.get(&key).await?;
            if let Some(data) = data {
                // Trust-but-verify: serving path re-checks the bytes against
                // the declared digest — a corrupted/mismatched store entry
                // fails closed instead of serving poison.
                // Kiểm tra lại khi phục vụ: bytes phải khớp digest khai —
                // entry hỏng/sai sẽ fail cứng thay vì phát dữ liệu bẩn.
                let actual = Self::compute_wire_digest(digest, &data)?;
                if actual != digest {
                    anyhow::bail!(
                        "blob integrity mismatch for '{digest}': bytes hash to '{actual}'"
                    );
                }
                return Ok(Some(data));
            }
            return Ok(None);
        }
        Ok(None)
    }

    /// Fetch tarball từ upstream (ITEM 4). None khi chưa cấu hình upstream,
    /// URL không cùng host upstream (chống SSRF) hoặc miss.
    pub async fn fetch_upstream_tarball(&self, tarball_url: &str) -> Result<Option<Vec<u8>>> {
        let Some(upstream) = &self.upstream else {
            return Ok(None);
        };
        let same_host = url::Url::parse(tarball_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .zip(
                url::Url::parse(&upstream.base)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned)),
            )
            .is_some_and(|(a, b)| a == b);
        if !same_host {
            return Ok(None);
        }
        upstream.fetch_bytes(tarball_url).await
    }

    /// Existence WITH disk awareness (audit vòng-3 P1-6): unlike the old
    /// boolean (which trusted the DB row + a bare path stat), this returns
    /// a TYPED status so a caller can never mistake "DB says present" for
    /// "present and trustworthy" — the same semantics asymmetry the audit
    /// flagged between blob_exists and the hardened serving path.
    ///
    /// - `Missing`: no DB row, or the backing file is gone.
    /// - `PresentLengthMatched`: file exists AND its length matches the
    ///   DB's recorded size (audit vòng-4 P1 rename — length only, NOT a
    ///   digest check; full digest verification happens at serve time in
    ///   get_blob — existence checks must stay cheap; a digest mismatch is
    ///   reported at serve with quarantine).
    /// - `PresentLengthMismatch`: file exists but its size disagrees with
    ///   the DB — almost certainly torn/corrupt; callers should treat this
    ///   as "needs repair", not as a cache hit.
    ///
    /// Existence có ý thức về đĩa (P1-6 audit vòng-3): khác boolean cũ (tin
    /// row DB + stat path trơ), hàm này trả status TYPED để caller không
    /// bao giờ nhầm "DB nói có" thành "có và đáng tin" — chính lệch semantics
    /// mà audit chỉ ra giữa blob_exists và đường serving đã harden.
    ///
    /// - `Missing`: không có row DB, hoặc file nền đã mất.
    /// - `PresentLengthMatched`: file tồn tại VÀ độ dài khớp size DB ghi
    ///   (đổi tên theo P1 audit vòng-4 — chỉ độ dài, KHÔNG phải check
    ///   digest; verify digest đầy đủ diễn ra lúc serve trong get_blob —
    ///   existence check phải giữ rẻ; lệch digest được báo lúc serve kèm
    ///   quarantine).
    /// - `PresentLengthMismatch`: file có nhưng size không khớp DB — gần
    ///   như chắc chắn đứt/hỏng; caller nên coi là "cần sửa", không phải
    ///   cache hit.
    pub async fn blob_exists(&self, digest: &str) -> Result<BlobPresence> {
        let row = sqlx::query("SELECT path, size FROM blobs WHERE digest = ?")
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        let Some(r) = row else {
            return Ok(BlobPresence::Missing);
        };
        let key: String = r.get("path");
        let db_size: i64 = r.get("size");

        match self.backend.exists(&key).await? {
            false => Ok(BlobPresence::Missing),
            true => {
                // Cheap integrity probe: length agreement with the DB record.
                // (Thăm dò integrity rẻ: độ dài khớp với bản ghi DB.)
                let actual_len = self.backend.published_len(&key).await?;
                if actual_len == db_size as u64 {
                    Ok(BlobPresence::PresentLengthMatched)
                } else {
                    Ok(BlobPresence::PresentLengthMismatch)
                }
            }
        }
    }

    /// Test-only: raw backend key for a stored digest (lets tests corrupt
    /// the blob file directly). Not part of the serving API.
    /// Chỉ dành cho test: key backend của digest đã lưu (để test sửa blob
    /// trực tiếp). Không thuộc serving API.
    #[doc(hidden)]
    pub async fn blob_path_for_test(&self, digest: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT path FROM blobs WHERE digest = ?")
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| r.get("path")))
    }

    /// Test-only: raw filesystem path for a stored OCI blob (lets tests
    /// corrupt the blob file directly, mirroring the npm blob test). Not
    /// part of the serving API.
    /// Chỉ dành cho test: path filesystem của blob OCI đã lưu (để test sửa
    /// blob trực tiếp, đối xứng với test blob npm). Không thuộc serving API.
    #[doc(hidden)]
    pub async fn oci_blob_path_for_test(&self, repo: &str, digest: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| r.get("path")))
    }

    // === OCI operations ===

    pub async fn put_oci_manifest(
        &self,
        repo: &str,
        reference: &str,
        manifest_bytes: &[u8],
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(manifest_bytes);
        let digest = format!("sha256:{}", hex::encode(hasher.finalize()));
        let manifest_json = String::from_utf8_lossy(manifest_bytes);
        sqlx::query(r#"
            INSERT INTO oci_manifests (repo, reference, manifest, digest)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(repo, reference) DO UPDATE SET manifest = excluded.manifest, digest = excluded.digest
        "#)
        .bind(repo)
        .bind(reference)
        .bind(manifest_json.as_ref())
        .bind(digest)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn get_oci_manifest(
        &self,
        repo: &str,
        reference: &str,
    ) -> Result<Option<crate::model::OciManifest>> {
        let row =
            sqlx::query("SELECT manifest FROM oci_manifests WHERE repo = ? AND reference = ?")
                .bind(repo)
                .bind(reference)
                .fetch_optional(&self.db)
                .await?;

        if let Some(row) = row {
            let manifest_json: String = row.get("manifest");
            let manifest = serde_json::from_str(&manifest_json)?;
            return Ok(Some(manifest));
        }

        // Fallback: reference may be a content digest
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Some(digest) = reference.strip_prefix("sha256:")
            && let Some(row) =
                sqlx::query("SELECT manifest FROM oci_manifests WHERE repo = ? AND digest = ?")
                    .bind(repo)
                    .bind(format!("sha256:{}", digest))
                    .fetch_optional(&self.db)
                    .await?
        {
            let manifest_json: String = row.get("manifest");
            let manifest = serde_json::from_str(&manifest_json)?;
            return Ok(Some(manifest));
        }
        Ok(None)
    }

    /// Raw manifest bytes + stored content digest (as pushed by the client).
    pub async fn get_oci_manifest_raw(
        &self,
        repo: &str,
        reference: &str,
    ) -> Result<Option<(String, String)>> {
        let row = sqlx::query(
            "SELECT manifest, digest FROM oci_manifests WHERE repo = ? AND reference = ?",
        )
        .bind(repo)
        .bind(reference)
        .fetch_optional(&self.db)
        .await?;

        if let Some(row) = row {
            return Ok(Some((row.get("manifest"), row.get("digest"))));
        }

        // Fallback: reference may be a content digest
        if let Some(digest) = reference.strip_prefix("sha256:") {
            let row = sqlx::query(
                "SELECT manifest, digest FROM oci_manifests WHERE repo = ? AND digest = ?",
            )
            .bind(repo)
            .bind(format!("sha256:{}", digest))
            .fetch_optional(&self.db)
            .await?;
            if let Some(row) = row {
                return Ok(Some((row.get("manifest"), row.get("digest"))));
            }
        }
        Ok(None)
    }

    pub async fn delete_oci_manifest(&self, repo: &str, reference: &str) -> Result<()> {
        sqlx::query("DELETE FROM oci_manifests WHERE repo = ? AND reference = ?")
            .bind(repo)
            .bind(reference)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn put_oci_blob(&self, repo: &str, digest: &str, data: &[u8]) -> Result<()> {
        // Repo is an OCI path segment — validate before joining the FS.
        // Repo là path segment OCI — validate trước khi join vào filesystem.
        validate_fs_segment(repo)?;
        let repo_dir = self.blobs_dir.join("oci").join(repo);
        fs::create_dir_all(&repo_dir).await?;

        let (p1, p2) = digest_hex_path(digest)?;
        let path = repo_dir.join(&p1).join(&p2);

        // Defense in depth: the HTTP handler already verifies the digest,
        // but the storage API recomputes it too — a future caller that
        // forgets cannot poison the blob store (P0-2).
        // Phòng vệ nhiều lớp: HTTP handler đã verify digest, nhưng storage
        // API tự tính lại — caller tương lai quên verify không thể đầu độc
        // blob store (P0-2).
        let actual = Self::compute_wire_digest(digest, data)?;
        if actual != digest {
            anyhow::bail!("oci digest mismatch: declared '{digest}' but bytes hash to '{actual}'");
        }
        if data.len() as u64 > max_oci_blob_bytes() {
            anyhow::bail!("OCI blob exceeds configured maximum size");
        }

        let parent = path
            .parent()
            .context("OCI blob path has no parent directory")?;
        fs::create_dir_all(parent).await?;

        // Atomic write: unique temp + rename. Concurrent pushes of the same
        // digest cannot leave a torn blob at the final path.
        // Ghi nguyên tử: temp duy nhất + rename. Push song song cùng digest
        // không thể để lại blob nửa vời ở path cuối.
        let tmp = parent.join(format!(
            ".upload-{}-{}",
            uuid::Uuid::new_v4(),
            std::process::id()
        ));
        let mut file = fs::File::create(&tmp).await?;
        file.write_all(data).await?;
        file.flush().await?;
        let sync = file.sync_all().await;
        drop(file);
        sync?;
        fs::rename(&tmp, &path).await?;

        sqlx::query(
            r#"
            INSERT INTO oci_blobs (repo, digest, size, path)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(repo, digest) DO UPDATE SET size = excluded.size, path = excluded.path
        "#,
        )
        .bind(repo)
        .bind(digest)
        .bind(data.len() as i64)
        .bind(path.to_string_lossy().to_string())
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Serve an OCI blob with integrity verification (P0-6): the bytes are
    /// re-hashed (SHA-256) and compared to the declared digest BEFORE they
    /// reach the client. A corrupted/tampered on-disk blob is quarantined
    /// for forensics and reported as a hard error — poison is never served.
    ///
    /// Phục vụ blob OCI có kiểm tra tính toàn vẹn (P0-6): bytes được rehash
    /// (SHA-256) và so với digest khai TRƯỚC khi tới client. Blob hỏng/sửa
    /// trên đĩa bị cách ly để điều tra và báo lỗi cứng — không bao giờ serve
    /// dữ liệu bẩn.
    pub async fn get_oci_blob(&self, repo: &str, digest: &str) -> Result<Option<Vec<u8>>> {
        // Validate the repo segment + digest BEFORE touching the DB/FS.
        // Validate repo segment + digest TRƯỚC khi chạm DB/FS.
        validate_fs_segment(repo)?;

        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let path: String = row.get("path");
        let data = fs::read(&path).await?;

        // Recompute the digest on the serving path — same algorithm the
        // put path used, so a store-entry mismatch fails closed here.
        // Tính lại digest trên đường serve — cùng thuật toán với đường put,
        // entry lệch sẽ fail cứng tại đây.
        let actual = Self::compute_wire_digest(digest, &data)?;
        if actual != digest {
            // Quarantine the corrupt blob for forensics (best-effort) and
            // fail closed — the client must not receive tampered bytes.
            // Cách ly blob hỏng để điều tra (best-effort) rồi fail cứng —
            // client không được nhận bytes đã bị sửa.
            quarantine_corrupt_blob(&path, digest, &actual)?;
            anyhow::bail!(
                "oci blob integrity mismatch for '{digest}' in repo '{repo}': bytes hash to '{actual}'"
            );
        }
        Ok(Some(data))
    }

    /// OCI blob existence with on-disk verification (P0-6): a DB row whose
    /// backing file is gone or unreadable reports false (fail-closed
    /// existence — clients get an honest 404 instead of trusting stale DB
    /// state).
    ///
    /// Kiểm tra tồn tại blob OCI có verify trên đĩa (P0-6): row DB mà file
    /// nền mất/đọc không được thì trả false (existence fail-closed — client
    /// nhận 404 trung thực thay vì tin DB stale).
    pub async fn oci_blob_exists(&self, repo: &str, digest: &str) -> Result<bool> {
        validate_fs_segment(repo)?;

        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;

        match row {
            Some(r) => {
                let path: String = r.get("path");
                // Trust-but-verify: the DB says it exists; the disk must
                // agree (metadata read, not full rehash — rehash happens on
                // serve in get_oci_blob).
                // Tin-nhưng-kiểm-tra: DB nói có; đĩa phải đồng ý (đọc
                // metadata, không rehash full — rehash xảy ra lúc serve
                // trong get_oci_blob).
                Ok(std::path::Path::new(&path).exists())
            }
            None => Ok(false),
        }
    }

    pub async fn delete_oci_blob(&self, repo: &str, digest: &str) -> Result<bool> {
        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        if let Some(row) = row {
            let path: String = row.get("path");
            let _ = fs::remove_file(&path).await; // file shared với repo khác — bỏ qua lỗi
            sqlx::query("DELETE FROM oci_blobs WHERE repo = ? AND digest = ?")
                .bind(repo)
                .bind(digest)
                .execute(&self.db)
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Copy blob từ repo khác vào repo này (cross-repo mount) — trả false nếu chưa tồn tại
    pub async fn mount_oci_blob(
        &self,
        from_repo: &str,
        digest: &str,
        to_repo: &str,
    ) -> Result<bool> {
        validate_fs_segment(from_repo)?;
        validate_fs_segment(to_repo)?;
        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(from_repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        let Some(row) = row else { return Ok(false) };
        let src_path: String = row.get("path");

        let repo_dir = self.blobs_dir.join("oci").join(to_repo);
        let (p1, p2) = digest_hex_path(digest)?;
        let dest = repo_dir.join(&p1).join(&p2);
        let parent = dest
            .parent()
            .context("mounted OCI blob path has no parent directory")?;
        fs::create_dir_all(parent).await?;
        if !dest.exists() {
            fs::copy(&src_path, &dest).await?;
        }
        sqlx::query(
            r#"
            INSERT INTO oci_blobs (repo, digest, size, path)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(repo, digest) DO UPDATE SET path = excluded.path
        "#,
        )
        .bind(to_repo)
        .bind(digest)
        .bind(
            self.get_oci_blob_size(from_repo, digest)
                .await?
                .unwrap_or(0),
        )
        .bind(dest.to_string_lossy().to_string())
        .execute(&self.db)
        .await?;
        Ok(true)
    }

    async fn get_oci_blob_size(&self, repo: &str, digest: &str) -> Result<Option<i64>> {
        let size = sqlx::query_scalar::<_, i64>(
            "SELECT size FROM oci_blobs WHERE repo = ? AND digest = ?",
        )
        .bind(repo)
        .bind(digest)
        .fetch_optional(&self.db)
        .await?;
        Ok(size)
    }

    // === OCI upload sessions (chunked/resumable) ===

    /// Return the per-session lock used to serialize upload offset checks.
    /// Trả khóa theo session để tuần tự hóa kiểm tra offset upload.
    fn oci_upload_lock(
        &self,
        repo: &str,
        uuid: &str,
    ) -> Result<std::sync::Arc<tokio::sync::Mutex<()>>> {
        let mut locks = self
            .upload_locks
            .lock()
            .map_err(|_| anyhow::anyhow!("OCI upload lock map is poisoned"))?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        let key = (repo.to_owned(), uuid.to_owned());
        if let Some(lock) = locks.get(&key).and_then(|lock| lock.upgrade()) {
            return Ok(lock);
        }
        let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(key, std::sync::Arc::downgrade(&lock));
        Ok(lock)
    }

    pub async fn create_oci_upload(&self, repo: &str, uuid: &str) -> Result<PathBuf> {
        // Both segments reach the filesystem — validate fail-closed.
        // Cả hai segment đều chạm filesystem — validate fail-closed.
        validate_fs_segment(repo)?;
        validate_fs_segment(uuid)?;
        let dir = self.blobs_dir.join("oci").join(repo).join("uploads");
        fs::create_dir_all(&dir).await?;
        let path = dir.join(uuid);
        sqlx::query("INSERT OR REPLACE INTO oci_uploads (repo, uuid, path, offset_bytes) VALUES (?, ?, ?, 0)")
            .bind(repo)
            .bind(uuid)
            .bind(path.to_string_lossy().to_string())
            .execute(&self.db)
            .await?;
        Ok(path)
    }

    pub async fn append_oci_upload(
        &self,
        repo: &str,
        uuid: &str,
        expected_offset: i64,
        data: &[u8],
    ) -> std::result::Result<i64, OciUploadAppendError> {
        validate_fs_segment(repo)?;
        validate_fs_segment(uuid)?;
        if expected_offset < 0 || data.is_empty() {
            return Err(OciUploadAppendError::OffsetMismatch);
        }
        let session_lock = self.oci_upload_lock(repo, uuid)?;
        let _guard = session_lock.lock().await;
        let data_len = i64::try_from(data.len())?;
        let next_offset = expected_offset
            .checked_add(data_len)
            .ok_or_else(|| anyhow::anyhow!("OCI upload offset overflow"))?;
        validate_oci_blob_size(next_offset as u64, max_oci_blob_bytes())?;
        let mut transaction = self.db.begin().await?;
        let claim = sqlx::query(
            "UPDATE oci_uploads SET offset_bytes = ? WHERE repo = ? AND uuid = ? AND offset_bytes = ?",
        )
        .bind(next_offset)
        .bind(repo)
        .bind(uuid)
        .bind(expected_offset)
        .execute(&mut *transaction)
        .await?;
        if claim.rows_affected() != 1 {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM oci_uploads WHERE repo = ? AND uuid = ?)",
            )
            .bind(repo)
            .bind(uuid)
            .fetch_one(&mut *transaction)
            .await?;
            transaction.rollback().await?;
            return Err(if exists == 0 {
                OciUploadAppendError::SessionNotFound
            } else {
                OciUploadAppendError::OffsetMismatch
            });
        }
        let row = sqlx::query("SELECT path FROM oci_uploads WHERE repo = ? AND uuid = ?")
            .bind(repo)
            .bind(uuid)
            .fetch_optional(&mut *transaction)
            .await?;
        let Some(row) = row else {
            transaction.rollback().await?;
            return Err(OciUploadAppendError::SessionNotFound);
        };
        let path = PathBuf::from(row.get::<String, _>("path"));
        let file_len = match fs::metadata(&path).await {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && expected_offset == 0 => 0,
            Err(error) => return Err(error.into()),
        };
        if file_len < expected_offset as u64 {
            return Err(anyhow::anyhow!(
                "OCI upload file length is shorter than its stored offset"
            )
            .into());
        }
        if file_len > expected_offset as u64 {
            Self::truncate_oci_upload(&path, expected_offset as u64).await?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        if let Err(error) = file.write_all(data).await {
            drop(file);
            let _ = Self::truncate_oci_upload(&path, expected_offset as u64).await;
            let _ = transaction.rollback().await;
            return Err(error.into());
        }
        if let Err(error) = file.flush().await {
            drop(file);
            let _ = Self::truncate_oci_upload(&path, expected_offset as u64).await;
            let _ = transaction.rollback().await;
            return Err(error.into());
        }
        if let Err(error) = file.sync_data().await {
            drop(file);
            let _ = Self::truncate_oci_upload(&path, expected_offset as u64).await;
            let _ = transaction.rollback().await;
            return Err(error.into());
        }
        drop(file);
        if fs::metadata(&path).await?.len() != next_offset as u64 {
            let _ = Self::truncate_oci_upload(&path, expected_offset as u64).await;
            transaction.rollback().await?;
            return Err(
                anyhow::anyhow!("OCI upload chunk did not write the declared byte count").into(),
            );
        }
        transaction.commit().await?;
        Ok(next_offset)
    }

    async fn truncate_oci_upload(path: &Path, offset: u64) -> Result<()> {
        let file = fs::OpenOptions::new().write(true).open(path).await?;
        file.set_len(offset).await?;
        Ok(())
    }

    pub async fn oci_upload_path(&self, repo: &str, uuid: &str) -> Result<Option<PathBuf>> {
        let row = sqlx::query("SELECT path FROM oci_uploads WHERE repo = ? AND uuid = ?")
            .bind(repo)
            .bind(uuid)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| PathBuf::from(r.get::<String, _>("path"))))
    }

    pub async fn oci_upload_offset(&self, repo: &str, uuid: &str) -> Result<Option<i64>> {
        validate_fs_segment(repo)?;
        validate_fs_segment(uuid)?;
        let offset = sqlx::query_scalar::<_, i64>(
            "SELECT offset_bytes FROM oci_uploads WHERE repo = ? AND uuid = ?",
        )
        .bind(repo)
        .bind(uuid)
        .fetch_optional(&self.db)
        .await?;
        Ok(offset)
    }

    pub async fn finish_oci_upload(
        &self,
        repo: &str,
        uuid: &str,
        expected_offset: i64,
    ) -> Result<bool> {
        validate_fs_segment(repo)?;
        validate_fs_segment(uuid)?;
        let session_lock = self.oci_upload_lock(repo, uuid)?;
        let _guard = session_lock.lock().await;
        let path = self.oci_upload_path(repo, uuid).await?;
        let result =
            sqlx::query("DELETE FROM oci_uploads WHERE repo = ? AND uuid = ? AND offset_bytes = ?")
                .bind(repo)
                .bind(uuid)
                .bind(expected_offset)
                .execute(&self.db)
                .await?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        if let Some(path) = path {
            match fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(true)
    }

    /// Verify and publish an upload file with bounded memory, then remove its
    /// session while SQLite excludes concurrent appends from other instances.
    /// Xác minh và publish file upload với RAM giới hạn, rồi xóa session trong
    /// khi SQLite chặn append đồng thời từ instance khác.
    pub async fn finalize_oci_upload(
        &self,
        repo: &str,
        uuid: &str,
        expected_digest: &str,
    ) -> Result<u64> {
        validate_fs_segment(repo)?;
        validate_fs_segment(uuid)?;
        let session_lock = self.oci_upload_lock(repo, uuid)?;
        let _guard = session_lock.lock().await;
        let mut transaction = self.db.begin().await?;
        let row =
            sqlx::query("SELECT path, offset_bytes FROM oci_uploads WHERE repo = ? AND uuid = ?")
                .bind(repo)
                .bind(uuid)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| anyhow::anyhow!("OCI upload session not found"))?;
        let path = PathBuf::from(row.get::<String, _>("path"));
        let expected_offset: i64 = row.get("offset_bytes");
        let claimed =
            sqlx::query("DELETE FROM oci_uploads WHERE repo = ? AND uuid = ? AND offset_bytes = ?")
                .bind(repo)
                .bind(uuid)
                .bind(expected_offset)
                .execute(&mut *transaction)
                .await?;
        if claimed.rows_affected() != 1 {
            anyhow::bail!("OCI upload changed while it was being finalized");
        }
        let size = fs::metadata(&path).await?.len();
        if expected_offset < 0 || size != expected_offset as u64 || size > max_oci_blob_bytes() {
            anyhow::bail!("OCI upload size or stored offset is invalid");
        }
        let written_size = self
            .publish_oci_blob_from_file(repo, expected_digest, &path, size, &mut transaction)
            .await?;
        transaction.commit().await?;
        match fs::remove_file(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(written_size)
    }

    async fn publish_oci_blob_from_file(
        &self,
        repo: &str,
        digest: &str,
        source_path: &Path,
        expected_size: u64,
        transaction: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> Result<u64> {
        use sha2::Digest;

        validate_fs_segment(repo)?;
        let repo_dir = self.blobs_dir.join("oci").join(repo);
        let (p1, p2) = digest_hex_path(digest)?;
        let path = repo_dir.join(p1).join(p2);
        let parent = path.parent().context("OCI blob path has no parent")?;
        fs::create_dir_all(parent).await?;
        let tmp = parent.join(format!(
            ".upload-{}-{}",
            uuid::Uuid::new_v4(),
            std::process::id()
        ));
        let mut guard = TempGuard::new(&tmp);
        let mut source = fs::File::open(source_path).await?;
        let mut output = fs::File::create(&tmp).await?;
        let mut buffer = vec![0; 128 * 1024];
        let mut hasher = sha2::Sha256::new();
        let mut total = 0_u64;
        loop {
            let count = source.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| anyhow::anyhow!("OCI blob size overflow"))?;
            if total > max_oci_blob_bytes() {
                anyhow::bail!("OCI blob exceeds configured maximum size");
            }
            hasher.update(&buffer[..count]);
            output.write_all(&buffer[..count]).await?;
        }
        let actual = format!("sha256:{}", hex::encode(hasher.finalize()));
        if total != expected_size || actual != digest {
            anyhow::bail!("OCI uploaded blob size or digest does not match its declaration");
        }
        output.flush().await?;
        output.sync_all().await?;
        drop(output);
        fs::rename(&tmp, &path).await?;
        guard.disarm();
        sqlx::query(
            "INSERT INTO oci_blobs (repo, digest, size, path) VALUES (?, ?, ?, ?) ON CONFLICT(repo, digest) DO UPDATE SET size = excluded.size, path = excluded.path",
        )
        .bind(repo)
        .bind(digest)
        .bind(i64::try_from(total)?)
        .bind(path.to_string_lossy().to_string())
        .execute(&mut **transaction)
        .await?;
        Ok(total)
    }

    /// Verify a manifest descriptor without allocating a layer-sized buffer.
    /// Xác minh descriptor manifest mà không cấp phát buffer cỡ toàn bộ layer.
    pub async fn verify_oci_blob_descriptor(
        &self,
        repo: &str,
        digest: &str,
        expected_size: u64,
    ) -> Result<bool> {
        validate_fs_segment(repo)?;
        let row = sqlx::query("SELECT path, size FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let path: String = row.get("path");
        let stored_size: i64 = row.get("size");
        if stored_size < 0 || stored_size as u64 != expected_size {
            return Ok(false);
        }
        let metadata = match fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if metadata.len() != expected_size {
            return Ok(false);
        }
        let actual = hash_file_sha256(Path::new(&path)).await?;
        if format!("sha256:{actual}") != digest {
            quarantine_corrupt_blob(&path, digest, &format!("sha256:{actual}"))?;
            anyhow::bail!("OCI blob integrity mismatch for '{digest}' in repo '{repo}'");
        }
        Ok(true)
    }

    // === OCI tags + catalog ===

    pub async fn list_oci_tags(&self, repo: &str) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT reference FROM oci_manifests WHERE repo = ? AND reference NOT LIKE 'sha256:%'",
        )
        .bind(repo)
        .fetch_all(&self.db)
        .await?;
        Ok(rows)
    }

    pub async fn list_oci_repos(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT DISTINCT repo FROM (SELECT repo FROM oci_manifests UNION SELECT repo FROM oci_blobs)"
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows.iter().map(|r| r.get("repo")).collect())
    }

    // Search
    pub async fn search_packages(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<crate::model::SearchResultItem>> {
        let rows = sqlx::query(
            r#"
            SELECT name, description FROM packages 
            WHERE name LIKE ? AND private = 0
            LIMIT ? OFFSET ?
        "#,
        )
        .bind(format!("%{}%", query))
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.db)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let name: String = row.get("name");
            let description: Option<String> = row.get("description");

            results.push(crate::model::SearchResultItem {
                package: crate::model::SearchPackage {
                    name,
                    version: "latest".to_string(),
                    description,
                    keywords: None,
                    date: chrono::Utc::now().to_rfc3339(),
                    links: crate::model::SearchLinks {
                        npm: None,
                        homepage: None,
                        repository: None,
                        bugs: None,
                    },
                    publisher: crate::model::SearchPublisher {
                        username: "registry".to_string(),
                        email: None,
                    },
                },
                score: 1.0,
                search_score: 1.0,
            });
        }
        Ok(results)
    }

    // === Users (persist — 10-task-plan Phase 3: users phải sống qua restart) ===

    /// Load mọi user (token → User) từ DB — gọi lúc khởi động
    pub async fn load_users(&self) -> Result<Vec<(String, crate::auth::User)>> {
        let rows =
            sqlx::query("SELECT token, name, password, email, is_admin, role, scopes FROM users")
                .fetch_all(&self.db)
                .await?;
        let mut out = Vec::new();
        for row in rows {
            let scopes: Vec<String> = serde_json::from_str(&row.get::<String, _>("scopes"))?;
            let role: String = row.get("role");
            out.push((
                row.get("token"),
                crate::auth::User {
                    name: row.get("name"),
                    is_admin: row.get("is_admin"),
                    role: role.parse::<crate::auth::UserRole>().unwrap_or_default(),
                    scopes,
                    password: row.get("password"),
                    email: row.get("email"),
                },
            ));
        }
        Ok(out)
    }

    /// Insert atomically without replacing credentials — không ghi đè tài khoản đã có.
    pub async fn create_user(&self, token: &str, user: &crate::auth::User) -> Result<bool> {
        let result = sqlx::query(
            "INSERT INTO users (name, token, password, email, is_admin, role, scopes) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(name) DO NOTHING",
        )
        .bind(&user.name).bind(token).bind(&user.password).bind(&user.email)
        .bind(user.is_admin).bind(user.role.as_str()).bind(serde_json::to_string(&user.scopes)?)
        .execute(&self.db).await?;
        Ok(result.rows_affected() == 1)
    }

    /// Upsert user — token sinh ở client (adduser), lưu qua đây để sống qua restart
    pub async fn upsert_user(&self, token: &str, user: &crate::auth::User) -> Result<()> {
        let scopes_json = serde_json::to_string(&user.scopes)?;
        sqlx::query(
            r#"
            INSERT INTO users (name, token, password, email, is_admin, role, scopes)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                token = excluded.token,
                password = excluded.password,
                email = excluded.email,
                is_admin = excluded.is_admin,
                role = excluded.role,
                scopes = excluded.scopes
        "#,
        )
        .bind(&user.name)
        .bind(token)
        .bind(&user.password)
        .bind(&user.email)
        .bind(user.is_admin)
        .bind(user.role.as_str())
        .bind(&scopes_json)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn delete_user_by_name(&self, name: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM users WHERE name = ?")
            .bind(name)
            .execute(&self.db)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Revoke token (ITEM 6) — xóa user theo token
    pub async fn delete_user_by_token(&self, token: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM users WHERE token = ?")
            .bind(token)
            .execute(&self.db)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    // === PyPI files (PEP 691 simple API — ai/lib python qua registry chung) ===

    pub async fn get_pypi_file_digest(&self, name: &str, filename: &str) -> Result<Option<String>> {
        let name = crate::trusted::canonical_pypi_name(name)
            .map_err(|_| anyhow::anyhow!("invalid PyPI project name"))?;
        let row = sqlx::query("SELECT digest FROM pypi_files WHERE name = ? AND filename = ?")
            .bind(name)
            .bind(filename)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| r.get("digest")))
    }

    pub async fn get_pypi_files(&self, name: &str) -> Result<Vec<crate::model::PypiFile>> {
        let name = crate::trusted::canonical_pypi_name(name)
            .map_err(|_| anyhow::anyhow!("invalid PyPI project name"))?;
        let rows = sqlx::query(
            r#"
            SELECT name, version, filename, digest, size, requires_python
            FROM pypi_files WHERE name = ? ORDER BY filename
        "#,
        )
        .bind(name)
        .fetch_all(&self.db)
        .await?;
        let mut out = Vec::new();
        for row in rows {
            out.push(crate::model::PypiFile {
                name: row.get("name"),
                version: row.get("version"),
                filename: row.get("filename"),
                digest: row.get("digest"),
                size: row.get("size"),
                requires_python: row.get("requires_python"),
            });
        }
        Ok(out)
    }

    pub async fn put_pypi_file(&self, file: &crate::model::PypiFile) -> Result<()> {
        let name = crate::trusted::canonical_pypi_name(&file.name)
            .map_err(|_| anyhow::anyhow!("invalid PyPI project name"))?;
        sqlx::query(
            r#"
            INSERT INTO pypi_files (name, version, filename, digest, size, requires_python)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(name, filename) DO UPDATE SET
                version = excluded.version,
                digest = excluded.digest,
                size = excluded.size,
                requires_python = excluded.requires_python
        "#,
        )
        .bind(name)
        .bind(&file.version)
        .bind(&file.filename)
        .bind(&file.digest)
        .bind(file.size)
        .bind(&file.requires_python)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

async fn hash_file_sha256(path: &Path) -> Result<String> {
    use sha2::Digest;

    let mut file = fs::File::open(path).await?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Quarantine a corrupt registry blob (P0-6): move the tampered file into
/// `<blobs_dir>/../quarantine/` next to a forensic report. Best-effort — a
/// quarantine failure must not mask the integrity error being reported.
///
/// Cách ly blob registry hỏng (P0-6): chuyển file bị sửa vào
/// `<blobs_dir>/../quarantine/` kèm báo cáo điều tra. Best-effort — lỗi
/// cách ly không được che lỗi integrity đang báo.
fn quarantine_corrupt_blob(path: &str, expected: &str, actual: &str) -> Result<()> {
    let src = std::path::Path::new(path);
    // Walk up from the blob file to the STORE root (parent of blobs/):
    // <store>/blobs/oci/<repo>/<p1>/<blob> → up 4 levels = <store>.
    // The OCI blob layout is blobs/oci/<repo>/<p1>/<p2>; the local npm
    // layout is blobs/<p1>/<p2> — resolve robustly by walking until the
    // directory named "blobs" is found, then use its parent.
    //
    // Đi lên từ file blob tới GỐC STORE (cha của blobs/): layout OCI là
    // blobs/oci/<repo>/<p1>/<p2>, layout npm là blobs/<p1>/<p2> — đi lên
    // tới khi gặp thư mục tên "blobs" rồi lấy cha của nó (ổn cho cả hai
    // layout).
    let mut cur = src.parent();
    while let Some(dir) = cur {
        if dir.file_name().map(|n| n == "blobs").unwrap_or(false) {
            let store_root = dir.parent().unwrap_or(dir);
            let qdir = store_root.join("quarantine");
            let _ = std::fs::create_dir_all(&qdir);
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let prefix: String = expected.chars().take(12).collect();
            let qfile = qdir.join(format!("corrupt-oci-{prefix}-{stamp}.blob"));
            let _ = std::fs::rename(src, &qfile);
            let _ = std::fs::write(
                qdir.join(format!("corrupt-oci-{prefix}-{stamp}.report")),
                format!(
                    "expected digest: {expected}\nactual digest:   {actual}\nsource path: {path}\nquarantined at: {stamp}\n"
                ),
            );
            return Ok(());
        }
        cur = dir.parent();
    }
    Ok(())
}

#[cfg(test)]
mod oci_upload_size_tests {
    use super::{OciUploadAppendError, validate_oci_blob_size};

    #[test]
    fn oci_upload_limit_accepts_boundary_and_rejects_next_byte() {
        assert!(validate_oci_blob_size(8, 8).is_ok());
        assert!(matches!(
            validate_oci_blob_size(9, 8),
            Err(OciUploadAppendError::TooLarge)
        ));
    }
}

#[cfg(test)]
#[path = "test/storage_migration.rs"]
mod storage_migration_tests;
