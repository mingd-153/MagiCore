//! Storage backend for registry server
//! (Storage: SQLite for metadata, filesystem for blobs)

use anyhow::{Context, Result};
use base64::Engine;
use serde_json;
use sqlx::{Pool, Row, Sqlite, SqlitePool};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Hex fragment của digest để làm path component an toàn.
/// - "sha512-<b64>": decode base64 → hex
/// - chuỗi hex thuần (vd "sha256:<hex>", hoặc hex không prefix): dùng trực tiếp
///
/// (Không slice raw b64 — b64 có thể bắt đầu `/`/`+` khiến Path::join tạo path
/// tuyệt đối, vd join("/+") → "/+/..." → tạo thư mục ở root → 500.)
fn digest_hex_path(digest: &str) -> (String, String) {
    let body = digest
        .rfind(['-', ':'])
        .map(|i| &digest[i + 1..])
        .unwrap_or(digest);
    let bytes = if body.bytes().all(|c| c.is_ascii_hexdigit()) {
        (0..body.len() / 2)
            .filter_map(|i| u8::from_str_radix(&body[i * 2..i * 2 + 2], 16).ok())
            .collect::<Vec<u8>>()
    } else {
        base64::engine::general_purpose::STANDARD
            .decode(body)
            .unwrap_or_default()
    };
    if bytes.is_empty() {
        return ("00".into(), String::new());
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if hex.len() < 4 {
        return ("00".into(), hex);
    }
    (hex[..2].to_string(), hex[2..].to_string())
}

/// Registry storage backend
pub struct RegistryStore {
    db: Pool<Sqlite>,
    blobs_dir: PathBuf,
    upstream: Option<Upstream>,
    backend: BlobBackend,
}

/// Blob storage backend (ITEM 5): Local FS hoặc S3-compatible (object_store).
pub enum BlobBackend {
    Local(PathBuf),
    S3(std::sync::Arc<object_store::aws::AmazonS3>),
}

use object_store::ObjectStore as _;

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

    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        match self {
            BlobBackend::Local(dir) => {
                let path = dir.join(key);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).await?;
                }
                let mut file = fs::File::create(&path).await?;
                file.write_all(data).await?;
                file.flush().await?;
                Ok(())
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
}

/// Upstream proxy (ITEM 4): GET miss → fetch từ registry upstream → cache local.
pub struct Upstream {
    base: String,
    client: reqwest::Client,
}

impl Upstream {
    pub fn new(base: String) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .user_agent("magicore-registry/0.1")
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
        Ok(Some(resp.json().await?))
    }

    async fn fetch_bytes(&self, url: &str) -> Result<Option<Vec<u8>>> {
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        Ok(Some(resp.bytes().await?.to_vec()))
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
        };

        store.init_schema().await?;

        Ok(store)
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
        let _pkg_json = serde_json::to_string(pkg)?;
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
        .execute(&self.db)
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
            .execute(&self.db)
            .await?;
        }

        Ok(())
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

    pub async fn put_blob(&self, digest: &str, data: &[u8]) -> Result<()> {
        let (p1, p2) = digest_hex_path(digest);
        let key = format!("{p1}/{p2}");
        self.backend.put(&key, data).await?;

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
        let row = sqlx::query("SELECT path FROM blobs WHERE digest = ?")
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;

        if let Some(row) = row {
            let key: String = row.get("path");
            self.backend.get(&key).await
        } else {
            Ok(None)
        }
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

    pub async fn blob_exists(&self, digest: &str) -> Result<bool> {
        let row = sqlx::query("SELECT path FROM blobs WHERE digest = ?")
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        match row {
            Some(r) => {
                let key: String = r.get("path");
                self.backend.exists(&key).await
            }
            None => Ok(false),
        }
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
        if let Some(digest) = reference.strip_prefix("sha256:") {
            if let Some(row) =
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
        let repo_dir = self.blobs_dir.join("oci").join(repo);
        fs::create_dir_all(&repo_dir).await?;

        let (p1, p2) = digest_hex_path(digest);
        let path = repo_dir.join(&p1).join(&p2);
        let parent = path
            .parent()
            .context("OCI blob path has no parent directory")?;
        fs::create_dir_all(parent).await?;

        let mut file = fs::File::create(&path).await?;
        file.write_all(data).await?;
        file.flush().await?;

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

    pub async fn get_oci_blob(&self, repo: &str, digest: &str) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;

        if let Some(row) = row {
            let path: String = row.get("path");
            let data = fs::read(path).await?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    pub async fn oci_blob_exists(&self, repo: &str, digest: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM oci_blobs WHERE repo = ? AND digest = ?",
        )
        .bind(repo)
        .bind(digest)
        .fetch_one(&self.db)
        .await?;
        Ok(count > 0)
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
        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(from_repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        let Some(row) = row else { return Ok(false) };
        let src_path: String = row.get("path");

        let repo_dir = self.blobs_dir.join("oci").join(to_repo);
        let (p1, p2) = digest_hex_path(digest);
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

    pub async fn create_oci_upload(&self, repo: &str, uuid: &str) -> Result<PathBuf> {
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

    pub async fn append_oci_upload(&self, repo: &str, uuid: &str, data: &[u8]) -> Result<i64> {
        let Some(path) = self.oci_upload_path(repo, uuid).await? else {
            return Err(anyhow::anyhow!("upload session not found"));
        };
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(data).await?;
        file.flush().await?;
        let offset = fs::metadata(&path).await?.len() as i64;
        sqlx::query("UPDATE oci_uploads SET offset_bytes = ? WHERE repo = ? AND uuid = ?")
            .bind(offset)
            .bind(repo)
            .bind(uuid)
            .execute(&self.db)
            .await?;
        Ok(offset)
    }

    pub async fn oci_upload_path(&self, repo: &str, uuid: &str) -> Result<Option<PathBuf>> {
        let row = sqlx::query("SELECT path FROM oci_uploads WHERE repo = ? AND uuid = ?")
            .bind(repo)
            .bind(uuid)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| PathBuf::from(r.get::<String, _>("path"))))
    }

    pub async fn finish_oci_upload(&self, repo: &str, uuid: &str) -> Result<()> {
        let _ = sqlx::query("DELETE FROM oci_uploads WHERE repo = ? AND uuid = ?")
            .bind(repo)
            .bind(uuid)
            .execute(&self.db)
            .await?;
        Ok(())
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
        let row = sqlx::query("SELECT digest FROM pypi_files WHERE name = ? AND filename = ?")
            .bind(name)
            .bind(filename)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| r.get("digest")))
    }

    pub async fn get_pypi_files(&self, name: &str) -> Result<Vec<crate::model::PypiFile>> {
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
        .bind(&file.name)
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
