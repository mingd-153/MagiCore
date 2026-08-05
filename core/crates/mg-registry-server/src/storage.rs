//! Storage backend for registry server
//! (Storage: SQLite for metadata, filesystem for blobs)

use anyhow::{Context, Result};
use serde_json;
use sqlx::{Pool, Sqlite, SqlitePool, Row};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Registry storage backend
pub struct RegistryStore {
    db: Pool<Sqlite>,
    blobs_dir: PathBuf,
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
        let pool = SqlitePool::connect(&db_url).await
            .context("Failed to connect to SQLite")?;
        
        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await
            .context("Failed to run migrations")?;
        
        let store = Self {
            db: pool,
            blobs_dir,
        };
        
        store.init_schema().await?;
        
        Ok(store)
    }
    
    async fn init_schema(&self) -> Result<()> {
        // Packages table
        sqlx::query(r#"
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
        "#).execute(&self.db).await?;
        
        // Package versions table
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS package_versions (
                id TEXT PRIMARY KEY,
                package_name TEXT NOT NULL,
                version TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE
            )
        "#).execute(&self.db).await?;
        
        // Blobs table (for OCI)
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS blobs (
                digest TEXT PRIMARY KEY,
                size INTEGER NOT NULL,
                path TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#).execute(&self.db).await?;
        
        // OCI manifests table
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS oci_manifests (
                repo TEXT NOT NULL,
                reference TEXT NOT NULL,
                manifest TEXT NOT NULL,
                digest TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (repo, reference)
            )
        "#).execute(&self.db).await?;
        
        // OCI blobs table
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS oci_blobs (
                repo TEXT NOT NULL,
                digest TEXT NOT NULL,
                size INTEGER NOT NULL,
                path TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (repo, digest)
            )
        "#).execute(&self.db).await?;
        
        // OCI upload sessions (chunked/resumable)
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS oci_uploads (
                repo TEXT NOT NULL,
                uuid TEXT NOT NULL,
                path TEXT NOT NULL,
                offset_bytes INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (repo, uuid)
            )
        "#).execute(&self.db).await?;
        
        Ok(())
    }
    
    // === Package operations ===
    
    pub async fn get_package(&self, name: &str) -> Result<Option<crate::model::Package>> {
        let row = sqlx::query(r#"
            SELECT name, description, dist_tags, maintainers, time, private
            FROM packages WHERE name = ?
        "#)
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
        } else {
            Ok(None)
        }
    }
    
    pub async fn get_package_versions(&self, name: &str) -> Result<std::collections::HashMap<String, crate::model::PackageVersion>> {
        let rows = sqlx::query(r#"
            SELECT version, data FROM package_versions WHERE package_name = ?
        "#)
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
        
        sqlx::query(r#"
            INSERT INTO packages (name, description, dist_tags, maintainers, time, private)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                description = excluded.description,
                dist_tags = excluded.dist_tags,
                maintainers = excluded.maintainers,
                time = excluded.time,
                private = excluded.private,
                updated_at = datetime('now')
        "#)
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
            sqlx::query(r#"
                INSERT INTO package_versions (id, package_name, version, data)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET data = excluded.data
            "#)
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
        let path = self.blobs_dir.join(&digest[7..9]).join(&digest[9..]);
        fs::create_dir_all(path.parent().unwrap()).await?;
        
        let mut file = fs::File::create(&path).await?;
        file.write_all(data).await?;
        file.flush().await?;
        
        sqlx::query(r#"
            INSERT INTO blobs (digest, size, path)
            VALUES (?, ?, ?)
            ON CONFLICT(digest) DO UPDATE SET size = excluded.size, path = excluded.path
        "#)
        .bind(digest)
        .bind(data.len() as i64)
        .bind(path.to_string_lossy().to_string())
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
            let path: String = row.get("path");
            let data = fs::read(path).await?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }
    
    pub async fn blob_exists(&self, digest: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM blobs WHERE digest = ?")
            .bind(digest)
            .fetch_one(&self.db)
            .await?;
        Ok(count > 0)
    }
    
    // === OCI operations ===
    
    pub async fn put_oci_manifest(&self, repo: &str, reference: &str, manifest_bytes: &[u8]) -> Result<()> {
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
    
    pub async fn get_oci_manifest(&self, repo: &str, reference: &str) -> Result<Option<crate::model::OciManifest>> {
        let row = sqlx::query("SELECT manifest FROM oci_manifests WHERE repo = ? AND reference = ?")
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
            if let Some(row) = sqlx::query("SELECT manifest FROM oci_manifests WHERE repo = ? AND digest = ?")
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
    pub async fn get_oci_manifest_raw(&self, repo: &str, reference: &str) -> Result<Option<(String, String)>> {
        let row = sqlx::query("SELECT manifest, digest FROM oci_manifests WHERE repo = ? AND reference = ?")
            .bind(repo)
            .bind(reference)
            .fetch_optional(&self.db)
            .await?;

        if let Some(row) = row {
            return Ok(Some((row.get("manifest"), row.get("digest"))));
        }

        // Fallback: reference may be a content digest
        if let Some(digest) = reference.strip_prefix("sha256:") {
            let row = sqlx::query("SELECT manifest, digest FROM oci_manifests WHERE repo = ? AND digest = ?")
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
        let repo_dir = self.blobs_dir.join("oci").join(&repo);
        fs::create_dir_all(&repo_dir).await?;
        
        let path = repo_dir.join(&digest[7..9]).join(&digest[9..]);
        fs::create_dir_all(path.parent().unwrap()).await?;
        
        let mut file = fs::File::create(&path).await?;
        file.write_all(data).await?;
        file.flush().await?;
        
        sqlx::query(r#"
            INSERT INTO oci_blobs (repo, digest, size, path)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(repo, digest) DO UPDATE SET size = excluded.size, path = excluded.path
        "#)
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
            "SELECT COUNT(*) FROM oci_blobs WHERE repo = ? AND digest = ?"
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
    pub async fn mount_oci_blob(&self, from_repo: &str, digest: &str, to_repo: &str) -> Result<bool> {
        let row = sqlx::query("SELECT path FROM oci_blobs WHERE repo = ? AND digest = ?")
            .bind(from_repo)
            .bind(digest)
            .fetch_optional(&self.db)
            .await?;
        let Some(row) = row else { return Ok(false) };
        let src_path: String = row.get("path");

        let repo_dir = self.blobs_dir.join("oci").join(to_repo);
        let dest = repo_dir.join(&digest[7..9]).join(&digest[9..]);
        fs::create_dir_all(dest.parent().unwrap()).await?;
        if !dest.exists() {
            fs::copy(&src_path, &dest).await?;
        }
        sqlx::query(r#"
            INSERT INTO oci_blobs (repo, digest, size, path)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(repo, digest) DO UPDATE SET path = excluded.path
        "#)
        .bind(to_repo)
        .bind(digest)
        .bind(self.get_oci_blob_size(from_repo, digest).await?.unwrap_or(0))
        .bind(dest.to_string_lossy().to_string())
        .execute(&self.db)
        .await?;
        Ok(true)
    }

    async fn get_oci_blob_size(&self, repo: &str, digest: &str) -> Result<Option<i64>> {
        let size = sqlx::query_scalar::<_, i64>(
            "SELECT size FROM oci_blobs WHERE repo = ? AND digest = ?"
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
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&path).await?;
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
            "SELECT reference FROM oci_manifests WHERE repo = ? AND reference NOT LIKE 'sha256:%'"
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
    pub async fn search_packages(&self, query: &str, limit: u32, offset: u32) -> Result<Vec<crate::model::SearchResultItem>> {
        let rows = sqlx::query(r#"
            SELECT name, description FROM packages 
            WHERE name LIKE ? AND private = 0
            LIMIT ? OFFSET ?
        "#)
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
        let rows = sqlx::query("SELECT token, name, password, email, is_admin, scopes FROM users")
            .fetch_all(&self.db)
            .await?;
        let mut out = Vec::new();
        for row in rows {
            let scopes: Vec<String> = serde_json::from_str(&row.get::<String, _>("scopes"))?;
            out.push((
                row.get("token"),
                crate::auth::User {
                    name: row.get("name"),
                    is_admin: row.get("is_admin"),
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
        sqlx::query(r#"
            INSERT INTO users (name, token, password, email, is_admin, scopes)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                token = excluded.token,
                password = excluded.password,
                email = excluded.email,
                is_admin = excluded.is_admin,
                scopes = excluded.scopes
        "#)
        .bind(&user.name)
        .bind(token)
        .bind(&user.password)
        .bind(&user.email)
        .bind(user.is_admin)
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
        let rows = sqlx::query(r#"
            SELECT name, version, filename, digest, size, requires_python
            FROM pypi_files WHERE name = ? ORDER BY filename
        "#)
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
        sqlx::query(r#"
            INSERT INTO pypi_files (name, version, filename, digest, size, requires_python)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(name, filename) DO UPDATE SET
                version = excluded.version,
                digest = excluded.digest,
                size = excluded.size,
                requires_python = excluded.requires_python
        "#)
        .bind(&file.name)
        .bind(&file.version)
        .bind(&file.filename)
        .bind(&file.digest)
        .bind(file.size as i64)
        .bind(&file.requires_python)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
