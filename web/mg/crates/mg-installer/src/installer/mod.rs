use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use base64::Engine;
use dashmap::DashMap;
use mg_core::cffi::sha256;
use rayon::ThreadPool;
use reqwest::Client;
use rusqlite::Connection;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

use mg_linker::linker::{LinkerFactory, LinkerOptions, LinkerStrategy};
use mg_lockfile::Lockfile;
use mg_store::store::cas::ContentStore;
use mg_store::SqliteStore;

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
    pub linker_strategy: LinkerStrategy,
    pub gvs_root: PathBuf,
}

impl Default for InstallOptions {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            concurrency: 16,
            retries: 3,
            retry_delay_ms: 1000,
            store_path: home.join(".mg").join("store"),
            virtual_store_path: PathBuf::from(".mg"),
            hoisted_node_modules: false,
            hoist_pattern: vec!["*".to_string()],
            offline: false,
            dry_run: false,
            project_root: PathBuf::from("."),
            sqlite_path: home.join(".mg").join("mg.db"),
            jsonl_log: false,
            linker_strategy: LinkerStrategy::Hoisted,
            gvs_root: home.join(".mg").join("gvs").join("v1"),
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
    pub linked: usize,
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
    #[error("integrity mismatch for {package}: expected {expected}, actual {actual}")]
    IntegrityMismatch {
        package: String,
        expected: String,
        actual: String,
    },
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
    store: Arc<mg_store::ContentStore>,
    cache: Arc<mg_store::PackageCache>,
    progress_tx: mpsc::Sender<InstallProgress>,
    client: Arc<Client>,
    thread_pool: Arc<ThreadPool>,
    conn: Mutex<Connection>,
    package_files: Arc<DashMap<String, Vec<(String, String)>>>,
    package_bins: Arc<DashMap<String, Vec<(String, String)>>>,
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
            package_bins: self.package_bins.clone(),
        }
    }
}

impl Installer {
    pub fn new(
        options: InstallOptions,
        progress_tx: mpsc::Sender<InstallProgress>,
    ) -> std::io::Result<Self> {
        let store = Arc::new(mg_store::ContentStore::new(options.store_path.clone())?);
        let cache = Arc::new(mg_store::PackageCache::new(
            options.store_path.join("packages"),
            options.store_path.join("cache"),
        )?);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(64)
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        let thread_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(options.concurrency)
                .build()
                .map_err(|e| std::io::Error::other(e.to_string()))?,
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
            package_bins: Arc::new(DashMap::new()),
        })
    }

    fn init_sqlite(sqlite_path: &Path) -> std::io::Result<Connection> {
        if let Some(parent) = sqlite_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn =
            Connection::open(sqlite_path).map_err(|e| std::io::Error::other(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS refcounts (
                package_id TEXT PRIMARY KEY,
                refcount INTEGER NOT NULL DEFAULT 1
            );",
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(conn)
    }

    pub async fn link_packages(
        &self,
        packages: &[mg_linker::PackageLinkInfo],
    ) -> Result<mg_linker::LinkResult, InstallError> {
        let conn = Arc::new(Mutex::new(
            Connection::open(self.options.sqlite_path.as_path())
                .map_err(|e| InstallError::SqlError(e.to_string()))?,
        ));

        let linker_options = LinkerOptions {
            project_root: self.options.project_root.clone(),
            virtual_store_dir: self.options.virtual_store_path.clone(),
            global_virtual_store: false,
            hoist: self.options.hoisted_node_modules,
            hoist_pattern: self.options.hoist_pattern.clone(),
            symlinks: true,
            store_path: self.options.store_path.clone(),
            workspace: None,
            refcount_callback: Some(Arc::new(move |package_id: &str| {
                let conn = conn
                    .lock()
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                conn.execute(
                    "INSERT INTO refcounts (package_id, refcount) VALUES (?1, 1)
                     ON CONFLICT(package_id) DO UPDATE SET refcount = refcount + 1",
                    rusqlite::params![package_id],
                )
                .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(())
            }) as mg_linker::RefcountCallback),
            strategy: self.options.linker_strategy,
            gvs_root: self.options.gvs_root.clone(),
        };

        let index = SqliteStore::open(&self.options.sqlite_path, false)
            .map_err(|e| InstallError::SqlError(e.to_string()))?;

        let store = ContentStore::new(
            self.options.store_path.join("v2").join("CAS"),
            Box::new(index),
        )
        .map_err(|e| InstallError::LinkError(e.to_string()))?;

        let linker = LinkerFactory::create(linker_options, &store)
            .map_err(|e| InstallError::LinkError(e.to_string()))?;

        linker
            .link_all(packages, &store, &self.options.project_root)
            .map_err(|e| InstallError::LinkError(e.to_string()))
    }

    pub async fn install_lockfile(&self, lockfile: &Lockfile) -> InstallResult {
        let total = lockfile.packages.len();
        let download_sem = Arc::new(Semaphore::new(self.options.concurrency.saturating_mul(2).max(4)));
        let extract_sem = Arc::new(Semaphore::new((self.options.concurrency / 2).max(2)));
        let mut join_set = JoinSet::new();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let succeeded = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let skipped = Arc::new(AtomicUsize::new(0));
        let store_arc = self.store.clone();
        let cache_arc = self.cache.clone();
        let package_files_arc = self.package_files.clone();
        let package_bins_arc = self.package_bins.clone();
        let client_arc = self.client.clone();
        let thread_pool_arc = self.thread_pool.clone();
        let sqlite_path = self.options.sqlite_path.clone();
        let refcounts: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

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
                linked: 0,
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
            let package_bins = package_bins_arc.clone();
            let package = pkg.clone();
            let dl_sem_clone = download_sem.clone();
            let ex_sem_clone = extract_sem.clone();
            let errors_clone = errors.clone();
            let succeeded_clone = succeeded.clone();
            let failed_clone = failed.clone();
            let skipped_clone = skipped.clone();
            let thread_pool = thread_pool_arc.clone();
            let refcounts_clone = refcounts.clone();

            join_set.spawn(async move {
                let _dl_permit = dl_sem_clone
                    .acquire_owned()
                    .await
                    .unwrap_or_else(|_| panic!("download semaphore closed"));

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
                        .push(InstallError::DownloadFailed(
                            "empty tarball URL".to_string(),
                        ));
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
                let actual_sri: String;

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

                    let mut hasher = sha256::Hasher::new();
                    let mut downloaded = 0u64;
                    let mut last_progress_decade = 0u64;
                    let mut last_unknown_progress = std::time::Instant::now();
                    let mut stream = resp_result.bytes_stream();

                    loop {
                        match futures_util::StreamExt::next(&mut stream).await {
                            Some(Ok(chunk)) => {
                                hasher.update(&chunk);
                                if let Err(e) = file.write_all(&chunk).await {
                                    errors_clone
                                        .lock()
                                        .unwrap()
                                        .push(InstallError::StoreError(e.to_string()));
                                    failed_clone.fetch_add(1, Ordering::SeqCst);
                                    return;
                                }
                                downloaded += chunk.len() as u64;

                                let should_send = if let Some(t) = total {
                                    if let Some(decade) = (downloaded * 10).checked_div(t) {
                                        if decade > last_progress_decade || downloaded >= t {
                                            last_progress_decade = decade;
                                            true
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    last_unknown_progress.elapsed().as_millis() > 500
                                };

                                if should_send {
                                    if total.is_none() { last_unknown_progress = std::time::Instant::now(); }
                                    let _ = tx
                                        .send(InstallProgress {
                                            package: package_id.clone(),
                                            phase: InstallPhase::Downloading,
                                            bytes_downloaded: downloaded,
                                            total_bytes: total,
                                        })
                                        .await;
                                }
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
                    let raw_hash = hasher.final_raw();
                    let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(raw_hash);
                    actual_sri = format!("sha256-{}", b64);
                    break 'retry;
                }

                // Verify package integrity before extracting
                if let Some(ref expected) = package.integrity {
                    if &actual_sri != expected {
                        errors_clone
                            .lock()
                            .unwrap()
                            .push(InstallError::IntegrityMismatch {
                                package: package_id.clone(),
                                expected: expected.clone(),
                                actual: actual_sri.clone(),
                            });
                        failed_clone.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                }

                // Release download permit before acquiring extract permit
                drop(_dl_permit);

                let _ex_permit = ex_sem_clone
                    .acquire_owned()
                    .await
                    .unwrap_or_else(|_| panic!("extract semaphore closed"));

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
                        let extractor = mg_store::tarball::TarballExtractor::new();
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

                // Fix scoped package paths: DefinitelyTyped tarballs use {name}/ instead of package/
                // Some packages like @types/node use {name} v{version}/ prefix
                let entries = if let Some(slash_pos) = package.name.rfind('/') {
                    let unscoped = &package.name[slash_pos + 1..];
                    let prefix = format!("{}/", unscoped);

                    // Find the common first-level directory (if any) among all entries
                    let common_dir = entries
                        .iter()
                        .filter_map(|(p, _)| p.split('/').next())
                        .reduce(|a, b| if a == b { a } else { "" })
                        .and_then(|d| if d.is_empty() { None } else { Some(d.to_string()) });

                    // Case 1: entries are under {unscoped}/ (DefinitelyTyped standard)
                    if entries.iter().all(|(p, _)| {
                        p == unscoped || p.starts_with(&prefix)
                    }) {
                        let fixed_entries: Vec<_> = entries.into_iter().map(|(path, hash)| {
                            let fixed = path.strip_prefix(&prefix).unwrap_or(&path).to_string();
                            (fixed, hash)
                        }).collect();
                        let src_dir = extract_dir.join(unscoped);
                        if src_dir.is_dir() {
                            if let Ok(entries) = std::fs::read_dir(&src_dir) {
                                for entry in entries.flatten() {
                                    let file_name = entry.file_name();
                                    let dst = extract_dir.join(&file_name);
                                    if !dst.exists() {
                                        std::fs::rename(entry.path(), &dst).ok();
                                    }
                                }
                            }
                            std::fs::remove_dir(&src_dir).ok();
                        }
                        fixed_entries
                    // Case 2: entries share a common non-standard prefix dir (e.g. "node v22.20/")
                    } else if let Some(dir) = common_dir.as_deref() {
                        if dir != "package" {
                            let dir_prefix = format!("{}/", dir);
                            let fixed_entries: Vec<_> = entries.into_iter().map(|(path, hash)| {
                                let fixed = path.strip_prefix(&dir_prefix).unwrap_or(&path).to_string();
                                (fixed, hash)
                            }).collect();
                            let src_dir = extract_dir.join(dir);
                            if src_dir.is_dir() {
                                if let Ok(child_entries) = std::fs::read_dir(&src_dir) {
                                    for entry in child_entries.flatten() {
                                        let file_name = entry.file_name();
                                        let dst = extract_dir.join(&file_name);
                                        if !dst.exists() {
                                            std::fs::rename(entry.path(), &dst).ok();
                                        }
                                    }
                                }
                                std::fs::remove_dir(&src_dir).ok();
                            }
                            fixed_entries
                        } else {
                            entries
                        }
                    } else {
                        entries
                    }
                } else {
                    entries
                };

                thread_pool.install(|| {
                    for (ref rel_path, _) in &entries {
                        let src_path = extract_dir.join(rel_path);
                        if src_path.is_file() {
                            if let Err(e) = store.import_file(&src_path) {
                                errors_clone
                                    .lock()
                                    .unwrap()
                                    .push(InstallError::StoreError(e.to_string()));
                            }
                        }
                    }
                });

                package_files.insert(package_id.clone(), entries);

                // Extract bin entries from package.json
                let bin_entries = {
                    let pkg_json_path = extract_dir.join("package.json");
                    // Fallback for scoped packages where files may not have been moved
                    let pkg_json_path = if pkg_json_path.is_file() {
                        pkg_json_path
                    } else if let Some(slash_pos) = package.name.rfind('/') {
                        let unscoped = &package.name[slash_pos + 1..];
                        let fallback = extract_dir.join(unscoped).join("package.json");
                        if fallback.is_file() { fallback } else { pkg_json_path }
                    } else {
                        pkg_json_path
                    };
                    if pkg_json_path.is_file() {
                        match std::fs::read_to_string(&pkg_json_path) {
                            Ok(content) => {
                                match serde_json::from_str::<serde_json::Value>(&content) {
                                    Ok(json) => {
                                        let pkg_name = json.get("name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or(&package.name)
                                            .to_string();
                                        match json.get("bin") {
                                            Some(serde_json::Value::String(s)) => {
                                                vec![(pkg_name, s.clone())]
                                            }
                                            Some(serde_json::Value::Object(obj)) => {
                                                obj.iter()
                                                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                                                    .collect()
                                            }
                                            _ => vec![],
                                        }
                                    }
                                    Err(_) => vec![],
                                }
                            }
                            Err(_) => vec![],
                        }
                    } else {
                        vec![]
                    }
                };
                package_bins.insert(package_id.clone(), bin_entries);

                let _ = tx
                    .send(InstallProgress {
                        package: package_id.clone(),
                        phase: InstallPhase::Linking,
                        bytes_downloaded: 0,
                        total_bytes: None,
                    })
                    .await;

                {
                    refcounts_clone
                        .lock()
                        .unwrap()
                        .push(package_id.clone());
                }

                if options.jsonl_log {
                    let jsonl_path = options.project_root.join(".mg").join("install.jsonl");
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
                        let _ =
                            std::io::Write::write_all(&mut f, format!("{}\n", entry).as_bytes());
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

        while join_set.join_next().await.is_some() {}

        // Batch-insert refcounts in a single SQLite transaction
        {
            let refcount_list = refcounts.lock().unwrap();
            if !refcount_list.is_empty() {
                if let Ok(conn) = Connection::open(&sqlite_path) {
                    let _ = conn.execute("BEGIN TRANSACTION", []);
                    for package_id in refcount_list.iter() {
                        let _ = conn.execute(
                            "INSERT INTO refcounts (package_id, refcount) VALUES (?1, 1)
                             ON CONFLICT(package_id) DO UPDATE SET refcount = refcount + 1",
                            rusqlite::params![package_id],
                        );
                    }
                    let _ = conn.execute("COMMIT", []);
                }
            }
        }

        // Link packages into node_modules
        let mut result = InstallResult {
            total,
            succeeded: succeeded.load(Ordering::SeqCst),
            failed: failed.load(Ordering::SeqCst),
            skipped: skipped.load(Ordering::SeqCst),
            linked: 0,
            errors: Arc::try_unwrap(errors)
                .unwrap_or_else(|_| Mutex::new(vec![]))
                .into_inner()
                .unwrap_or_default(),
        };

        let succeeded_count = result.succeeded;
        if succeeded_count > 0 {
            let _ = self
                .progress_tx
                .send(InstallProgress {
                    package: "linking".to_string(),
                    phase: InstallPhase::Linking,
                    bytes_downloaded: 0,
                    total_bytes: None,
                })
                .await;

            let link_infos = self.collect_package_link_infos_with_deps(lockfile);
            match self.link_packages(&link_infos).await {
                Ok(link_result) => {
                    let _ = self
                        .progress_tx
                        .send(InstallProgress {
                            package: "linking".to_string(),
                            phase: InstallPhase::Done,
                            bytes_downloaded: 0,
                            total_bytes: None,
                        })
                        .await;
                    result.linked = link_result.linked.len();
                }
                Err(e) => {
                    result.errors.push(e);
                    result.failed += 1;
                }
            }
        }

        result
    }

    /// Collect link infos from package_files, enriched with lockfile dependency data
    pub fn collect_package_link_infos_with_deps(
        &self,
        lockfile: &Lockfile,
    ) -> Vec<mg_linker::PackageLinkInfo> {
        let mut infos = Vec::new();
        for entry in self.package_files.iter() {
            let package_id = entry.key();
            let files = entry.value();
            let at = package_id.rfind('@').unwrap_or(0);
            if at == 0 {
                continue;
            }
            let name = package_id[..at].to_string();
            let version = package_id[at + 1..].to_string();

            let deps = lockfile
                .packages
                .iter()
                .find(|p| p.name == name && p.version == version)
                .map(|p| p.dependencies.clone())
                .unwrap_or_default();

            let dep_specs = lockfile
                .packages
                .iter()
                .find(|p| p.name == name && p.version == version)
                .map(|p| p.dep_specs.clone())
                .unwrap_or_default();

            let bin_entries = self.package_bins
                .get(package_id.as_str())
                .map(|b| b.value().clone())
                .unwrap_or_default();

            infos.push(mg_linker::PackageLinkInfo {
                name,
                version,
                dependencies: deps,
                dep_specs,
                peer_dependencies: vec![],
                files: files.clone(),
                is_root_dep: false,
                bin_entries,
                total_size: 0,
                dep_graph_hash: String::new(),
            });
        }
        infos
    }

    pub fn collect_package_link_infos(&self) -> Vec<mg_linker::PackageLinkInfo> {
        let mut infos = Vec::new();
        for entry in self.package_files.iter() {
            let package_id = entry.key();
            let files = entry.value();
            let at = package_id.rfind('@').unwrap_or(0);
            if at == 0 {
                continue;
            }
            let name = package_id[..at].to_string();
            let version = package_id[at + 1..].to_string();
            infos.push(mg_linker::PackageLinkInfo {
                name,
                version,
                dependencies: vec![],
                dep_specs: vec![],
                peer_dependencies: vec![],
                files: files.clone(),
                is_root_dep: false,
                bin_entries: vec![],
                total_size: 0,
                dep_graph_hash: String::new(),
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
            let at = package_id.rfind('@').unwrap_or(0);
            if at > 0 {
                let name = &package_id[..at];
                let version = &package_id[at + 1..];
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
        let path = project_root.join(".mg").join("install.jsonl");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(&path)?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }

    pub fn log(&mut self, progress: &InstallProgress, package_id: &str) -> std::io::Result<()> {
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
