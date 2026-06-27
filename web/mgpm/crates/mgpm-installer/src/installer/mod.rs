use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use dashmap::DashMap;
use rayon::ThreadPool;
use reqwest::Client;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

use mgpm_lockfile::Lockfile;

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub concurrency: usize,
    pub retries: u32,
    pub retry_delay_ms: u64,
    pub store_path: PathBuf,
    pub virtual_store_path: PathBuf,
    pub hoisted_node_modules: bool,
    pub hoist_pattern: Vec<String>,
    pub offline: bool,
    pub dry_run: bool,
    pub project_root: PathBuf,
    pub sqlite_path: PathBuf,
    pub jsonl_log: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            concurrency: 16,
            retries: 3,
            retry_delay_ms: 1000,
            store_path: home.join(".mgpm").join("store"),
            virtual_store_path: PathBuf::from(".mgpm").join("virtual_store"),
            hoisted_node_modules: false,
            hoist_pattern: vec!["*".to_string()],
            offline: false,
            dry_run: false,
            project_root: PathBuf::from("."),
            sqlite_path: home.join(".mgpm").join("mgpm.db"),
            jsonl_log: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub package: String,
    pub phase: InstallPhase,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    Pending,
    Downloading,
    Extracting,
    Linking,
    Done,
    Failed,
    SkippedOffline,
    SkippedDryRun,
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: Vec<InstallError>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum InstallError {
    #[error("store error: {0}")]
    StoreError(String),
    #[error("package not in store")]
    PackageNotInStore,
    #[error("download failed: {0}")]
    DownloadFailed(String),
    #[error("tarball extract error: {0}")]
    TarballExtract(String),
    #[error("link error: {0}")]
    LinkError(String),
    #[error("SQL error: {0}")]
    SqlError(String),
    #[error("IO error: {0}")]
    Io(String),
}

impl From<std::io::Error> for InstallError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for InstallError {
    fn from(e: rusqlite::Error) -> Self {
        Self::SqlError(e.to_string())
    }
}

pub struct Installer {
    options: InstallOptions,
    store: Arc<mgpm_store::ContentStore>,
    cache: Arc<mgpm_store::PackageCache>,
    progress_tx: mpsc::Sender<InstallProgress>,
    client: Arc<Client>,
    thread_pool: Arc<ThreadPool>,
    conn: Mutex<Connection>,
    package_files: Arc<DashMap<String, Vec<(String, String)>>>,
}

impl Clone for Installer {
    fn clone(&self) -> Self {
        let conn = self.conn.lock().expect("conn lock");
        let conn_clone = Connection::open(self.options.sqlite_path.as_path())
            .expect("failed to clone SQLite connection");
        drop(conn);
        Self {
            options: self.options.clone(),
            store: self.store.clone(),
            cache: self.cache.clone(),
            progress_tx: self.progress_tx.clone(),
            client: self.client.clone(),
            thread_pool: self.thread_pool.clone(),
            conn: Mutex::new(conn_clone),
            package_files: self.package_files.clone(),
        }
    }
}

impl Installer {
    pub fn new(
        options: InstallOptions,
        progress_tx: mpsc::Sender<InstallProgress>,
    ) -> std::io::Result<Self> {
        let store = Arc::new(mgpm_store::ContentStore::new(options.store_path.clone())?);
        let cache = Arc::new(mgpm_store::PackageCache::new(
            options.store_path.join("packages"),
            options.store_path.join("cache"),
        )?);

        let client = Client::builder()
            .pool_max_idle_per_host(64)
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .http2_prior_knowledge()
            .build()
            .map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
            })?;

        let thread_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(options.concurrency)
                .build()
                .map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                })?,
        );

        let conn = Mutex::new(Self::init_sqlite(&options.sqlite_path)?);

        Ok(Self {
            options,
            store,
            cache,
            progress_tx,
            client: Arc::new(client),
            thread_pool,
            conn,
            package_files: Arc::new(DashMap::new()),
        })
    }

    fn init_sqlite(sqlite_path: &Path) -> std::io::Result<Connection> {
        if let Some(parent) = sqlite_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(sqlite_path).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS refcounts (
                package_id TEXT PRIMARY KEY,
                refcount INTEGER NOT NULL DEFAULT 1
            );",
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(conn)
    }

    pub async fn link_packages(
        &self,
        packages: &[mgpm_linker::PackageLinkInfo],
    ) -> Result<mgpm_linker::LinkResult, InstallError> {
        use mgpm_linker::Linker;

        let conn = Arc::new(Mutex::new(
            Connection::open(self.options.sqlite_path.as_path())
                .map_err(|e| InstallError::SqlError(e.to_string()))?,
        ));

        let linker_options = mgpm_linker::LinkerOptions {
            project_root: self.options.project_root.clone(),
            virtual_store_dir: self.options.virtual_store_path.clone(),
            global_virtual_store: false,
            hoist: self.options.hoisted_node_modules,
            hoist_pattern: self.options.hoist_pattern.clone(),
            symlinks: true,
            store_path: self.options.store_path.clone(),
            workspace: None,
            refcount_callback: Some(Arc::new(move |package_id: &str| {
                let conn = conn.lock().map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                })?;
                conn.execute(
                    "INSERT INTO refcounts (package_id, refcount) VALUES (?1, 1)
                     ON CONFLICT(package_id) DO UPDATE SET refcount = refcount + 1",
                    rusqlite::params![package_id],
                )
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
                Ok(())
            }) as mgpm_linker::RefcountCallback),
        };

        let linker = Linker::new(linker_options);
        linker
            .link_packages(packages)
            .map_err(|e| InstallError::LinkError(e.to_string()))
    }

    pub async fn install_lockfile(&self, lockfile: &Lockfile) -> InstallResult {
        let total = lockfile.packages.len();
        let semaphore = Arc::new(Semaphore::new(self.options.concurrency));
        let mut join_set = JoinSet::new();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let succeeded = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let skipped = Arc::new(AtomicUsize::new(0));
        let store_arc = self.store.clone();
        let cache_arc = self.cache.clone();
        let package_files_arc = self.package_files.clone();
        let client_arc = self.client.clone();
        let thread_pool_arc = self.thread_pool.clone();
        let sqlite_path = self.options.sqlite_path.clone();

        if self.options.dry_run {
            for pkg in &lockfile.packages {
                let _ = self
                    .progress_tx
                    .send(InstallProgress {
                        package: pkg.name.clone(),
                        phase: InstallPhase::SkippedDryRun,
                        bytes_downloaded: 0,
                        total_bytes: None,
                    })
                    .await;
            }
            return InstallResult {
                total,
                succeeded: 0,
                failed: 0,
                skipped: total,
                errors: vec![],
            };
        }

        for pkg in &lockfile.packages {
            let tx = self.progress_tx.clone();
            let options = self.options.clone();
            let store = store_arc.clone();
            let cache = cache_arc.clone();
            let client = client_arc.clone();
            let package_files = package_files_arc.clone();
            let package = pkg.clone();
            let sem_clone = semaphore.clone();
            let errors_clone = errors.clone();
            let succeeded_clone = succeeded.clone();
            let failed_clone = failed.clone();
            let skipped_clone = skipped.clone();
            let thread_pool = thread_pool_arc.clone();
            let sqlite = sqlite_path.clone();

            join_set.spawn(async move {
                let _permit = sem_clone
                    .acquire_owned()
                    .await
                    .unwrap_or_else(|_| panic!("semaphore closed"));

                let package_id = format!("{}@{}", package.name, package.version);
                let _ = tx
                    .send(InstallProgress {
                        package: package_id.clone(),
                        phase: InstallPhase::Pending,
                        bytes_downloaded: 0,
                        total_bytes: None,
                    })
                    .await;

                if cache.has_package(&package_id) && options.offline {
                    let _ = tx
                        .send(InstallProgress {
                            package: package_id.clone(),
                            phase: InstallPhase::SkippedOffline,
                            bytes_downloaded: 0,
                            total_bytes: None,
                        })
                        .await;
                    skipped_clone.fetch_add(1, Ordering::SeqCst);
                    return;
                }

                let tarball_url = package.resolution.url.clone();
                if tarball_url.is_empty() {
                    errors_clone
                        .lock()
                        .unwrap()
                        .push(InstallError::DownloadFailed("empty tarball URL".to_string()));
                    failed_clone.fetch_add(1, Ordering::SeqCst);
                    return;
                }

                let temp_dir = match tempfile::tempdir() {
                    Ok(d) => d,
                    Err(e) => {
                        errors_clone
                            .lock()
                            .unwrap()
                            .push(InstallError::StoreError(e.to_string()));
                        failed_clone.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                };
                let tarball_path = temp_dir.path().join("package.tgz");

                let max_retries = options.retries as usize;
                let retry_delay_ms = options.retry_delay_ms;
                let _hash: String;

                'retry: loop {
                    let mut retry_attempt = 0usize;

                    let resp_result = loop {
                        let result = client
                            .get(&tarball_url)
                            .send()
                            .await
                            .map_err(|e| InstallError::DownloadFailed(e.to_string()));

                        match result {
                            Ok(r) if r.status().is_success() => break r,
                            Ok(r) => {
                                let err = InstallError::DownloadFailed(format!(
                                    "HTTP {} for {}",
                                    r.status(),
                                    tarball_url
                                ));
                                if retry_attempt < max_retries {
                                    retry_attempt += 1;
                                    let delay = retry_delay_ms
                                        * 2u64.pow(retry_attempt.saturating_sub(1) as u32);
                                    tokio::time::sleep(std::time::Duration::from_millis(delay))
                                        .await;
                                    continue;
                                }
                                errors_clone.lock().unwrap().push(err);
                                failed_clone.fetch_add(1, Ordering::SeqCst);
                                return;
                            }
                            Err(e) => {
                                if retry_attempt < max_retries {
                                    retry_attempt += 1;
                                    let delay = retry_delay_ms
                                        * 2u64.pow(retry_attempt.saturating_sub(1) as u32);
                                    tokio::time::sleep(std::time::Duration::from_millis(delay))
                                        .await;
                                    continue;
                                }
                                errors_clone.lock().unwrap().push(e);
                                failed_clone.fetch_add(1, Ordering::SeqCst);
                                return;
                            }
                        }
                    };

                    let total = resp_result.content_length();
                    let mut file = match tokio::fs::File::create(&tarball_path).await {
                        Ok(f) => f,
                        Err(e) => {
                            errors_clone
                                .lock()
                                .unwrap()
                                .push(InstallError::StoreError(e.to_string()));
                            failed_clone.fetch_add(1, Ordering::SeqCst);
                            return;
                        }
                    };

                    let mut hasher = Sha256::new();
                    let mut downloaded = 0u64;
                    let mut stream = resp_result.bytes_stream();

                    loop {
                        match futures_util::StreamExt::next(&mut stream).await {
                            Some(Ok(chunk)) => {
                                hasher.update(&chunk);
                                if let Err(e) = file.write_all(&chunk).await {
                                    errors_clone.lock().unwrap().push(
                                        InstallError::StoreError(e.to_string()),
                                    );
                                    failed_clone.fetch_add(1, Ordering::SeqCst);
                                    return;
                                }
                                downloaded += chunk.len() as u64;
                                let _ = tx
                                    .send(InstallProgress {
                                        package: package_id.clone(),
                                        phase: InstallPhase::Downloading,
                                        bytes_downloaded: downloaded,
                                        total_bytes: total,
                                    })
                                    .await;
                            }
                            Some(Err(e)) => {
                                if retry_attempt < max_retries {
                                    retry_attempt += 1;
                                    let delay = retry_delay_ms
                                        * 2u64.pow(retry_attempt.saturating_sub(1) as u32);
                                    tokio::time::sleep(std::time::Duration::from_millis(delay))
                                        .await;
                                    let _ = tokio::fs::remove_file(&tarball_path).await;
                                    continue 'retry;
                                }
                                errors_clone
                                    .lock()
                                    .unwrap()
                                    .push(InstallError::DownloadFailed(e.to_string()));
                                failed_clone.fetch_add(1, Ordering::SeqCst);
                                return;
                            }
                            None => break,
                        }
                    }

                    if let Err(e) = file.flush().await {
                        errors_clone
                            .lock()
                            .unwrap()
                            .push(InstallError::StoreError(e.to_string()));
                        failed_clone.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                    _hash = hex::encode(hasher.finalize());
                    break 'retry;
                }

                let _ = tx
                    .send(InstallProgress {
                        package: package_id.clone(),
                        phase: InstallPhase::Extracting,
                        bytes_downloaded: 0,
                        total_bytes: None,
                    })
                    .await;

                let extract_dir = temp_dir.path().join("extracted");
                if let Err(e) = std::fs::create_dir_all(&extract_dir) {
                    errors_clone
                        .lock()
                        .unwrap()
                        .push(InstallError::StoreError(e.to_string()));
                    failed_clone.fetch_add(1, Ordering::SeqCst);
                    return;
                }

                let entries = tokio::task::spawn_blocking({
                    let tarball = tarball_path.clone();
                    let extract = extract_dir.clone();
                    move || {
                        let extractor = mgpm_store::tarball::TarballExtractor::new();
                        extractor
                            .extract(&tarball, &extract)
                            .map(|entries| {
                                entries
                                    .into_iter()
                                    .map(|e| (e.path, e.hash))
                                    .collect::<Vec<_>>()
                            })
                            .map_err(|e| InstallError::TarballExtract(e.to_string()))
                    }
                })
                .await
                .unwrap_or_else(|e| Err(InstallError::StoreError(e.to_string())));

                let entries = match entries {
                    Ok(e) => e,
                    Err(e) => {
                        errors_clone.lock().unwrap().push(e);
                        failed_clone.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                };

                thread_pool.install(|| {
                    for (ref rel_path, _) in &entries {
                        let src_path = extract_dir.join(rel_path);
                        if src_path.is_file() {
                            if let Err(e) = store.import_file(&src_path) {
                                errors_clone.lock().unwrap().push(
                                    InstallError::StoreError(e.to_string()),
                                );
                            }
                        }
                    }
                });

                package_files.insert(package_id.clone(), entries);

                let _ = tx
                    .send(InstallProgress {
                        package: package_id.clone(),
                        phase: InstallPhase::Linking,
                        bytes_downloaded: 0,
                        total_bytes: None,
                    })
                    .await;

                {
                    let conn = match Connection::open(&sqlite) {
                        Ok(c) => c,
                        Err(e) => {
                            errors_clone
                                .lock()
                                .unwrap()
                                .push(InstallError::SqlError(e.to_string()));
                            failed_clone.fetch_add(1, Ordering::SeqCst);
                            return;
                        }
                    };
                    if let Err(e) = conn.execute(
                        "INSERT INTO refcounts (package_id, refcount) VALUES (?1, 1)
                         ON CONFLICT(package_id) DO UPDATE SET refcount = refcount + 1",
                        rusqlite::params![package_id],
                    ) {
                        errors_clone
                            .lock()
                            .unwrap()
                            .push(InstallError::SqlError(e.to_string()));
                    }
                }

                if options.jsonl_log {
                    let jsonl_path = options.project_root.join(".mgpm").join("install.jsonl");
                    if let Some(parent) = jsonl_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&jsonl_path)
                    {
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let entry = serde_json::json!({
                            "timestamp": ts,
                            "package_id": package_id,
                            "phase": "Done",
                            "bytes_downloaded": 0,
                        });
                        let _ = std::io::Write::write_all(
                            &mut f,
                            format!("{}\n", entry).as_bytes(),
                        );
                    }
                }

                let _ = tx
                    .send(InstallProgress {
                        package: package_id.clone(),
                        phase: InstallPhase::Done,
                        bytes_downloaded: 0,
                        total_bytes: None,
                    })
                    .await;

                succeeded_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        while let Some(_) = join_set.join_next().await {}

        InstallResult {
            total,
            succeeded: succeeded.load(Ordering::SeqCst),
            failed: failed.load(Ordering::SeqCst),
            skipped: skipped.load(Ordering::SeqCst),
            errors: Arc::try_unwrap(errors)
                .unwrap_or_else(|_| Mutex::new(vec![]))
                .into_inner()
                .unwrap_or_default(),
        }
    }

    pub fn collect_package_link_infos(&self) -> Vec<mgpm_linker::PackageLinkInfo> {
        let mut infos = Vec::new();
        for entry in self.package_files.iter() {
            let package_id = entry.key();
            let files = entry.value();
            let parts: Vec<&str> = package_id.splitn(2, '@').collect();
            let (name, version) = if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                continue;
            };
            infos.push(mgpm_linker::PackageLinkInfo {
                name,
                version,
                dependencies: vec![],
                peer_dependencies: vec![],
                files: files.clone(),
            });
        }
        infos
    }

    pub fn refcount(&self, package_id: &str) -> Result<i64, InstallError> {
        let conn = self.conn.lock().expect("conn lock");
        let count: i64 = conn
            .query_row(
                "SELECT refcount FROM refcounts WHERE package_id = ?1",
                rusqlite::params![package_id],
                |row| row.get(0),
            )
            .map_err(|e| InstallError::SqlError(e.to_string()))?;
        Ok(count)
    }

    pub fn decrement_refcount(&self, package_id: &str) -> Result<(), InstallError> {
        let conn = Connection::open(self.options.sqlite_path.as_path())
            .map_err(|e| InstallError::SqlError(e.to_string()))?;
        conn.execute(
            "UPDATE refcounts SET refcount = MAX(0, refcount - 1) WHERE package_id = ?1",
            rusqlite::params![package_id],
        )?;
        let count: i64 = conn
            .query_row(
                "SELECT refcount FROM refcounts WHERE package_id = ?1",
                rusqlite::params![package_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if count <= 0 {
            conn.execute(
                "DELETE FROM refcounts WHERE package_id = ?1",
                rusqlite::params![package_id],
            )?;
            let parts: Vec<&str> = package_id.splitn(2, '@').collect();
            if parts.len() == 2 {
                let name = parts[0];
                let version = parts[1];
                let pkg_dir = self
                    .options
                    .store_path
                    .join("packages")
                    .join(format!("{}@{}", name, version));
                if pkg_dir.exists() {
                    std::fs::remove_dir_all(&pkg_dir).ok();
                }
            }
        }
        Ok(())
    }
}

pub struct JsonlLogger {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl JsonlLogger {
    pub fn new(project_root: &Path) -> std::io::Result<Self> {
        let path = project_root.join(".mgpm").join("install.jsonl");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(&path)?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }

    pub fn log(
        &mut self,
        progress: &InstallProgress,
        package_id: &str,
    ) -> std::io::Result<()> {
        if let Some(ref mut file) = self.file {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let entry = serde_json::json!({
                "timestamp": ts,
                "package": progress.package,
                "package_id": package_id,
                "phase": format!("{:?}", progress.phase),
                "bytes_downloaded": progress.bytes_downloaded,
                "total_bytes": progress.total_bytes,
            });
            use std::io::Write;
            writeln!(file, "{}", entry)?;
            file.flush()?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
