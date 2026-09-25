//! mgc store — manage the local package store (02 §2.2)
//! (Quản lý store: prune package không còn project nào tham chiếu)

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use mgc_config::project::ProjectConfig;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Subcommand, Debug, Clone)]
pub enum StoreCmd {
    /// Delete packages not referenced by any project (refcount == 0)
    Prune {
        #[arg(long, help = "report only, do not delete")]
        dry_run: bool,
        #[arg(long, help = "output JSON")]
        json: bool,
    },
    /// Show package store summary
    Status,
    /// Back up the store to a target directory
    Backup {
        /// Target directory (default: ~/.magicore/store_backup_<timestamp>)
        #[arg(long)]
        path: Option<String>,
    },
    /// Restore the store from a backup directory
    Restore {
        /// Backup directory path
        #[arg(long)]
        path: String,
    },
    /// Check store health (DB readability, CAS root safety) and report
    /// remediation steps — the target of every prune refusal (P1-6 +
    /// P0-C, Tech Lead vòng-7). `--repair` re-locks a quarantined store
    /// and clears stale temp files. EXIT CONTRACT (P0-B, adversarial
    /// review vòng-9): 0 = healthy (or repaired AND re-checked healthy),
    /// 1 = corruption/invariant violation — scripts and CI must be able
    /// to trust the code, `doctor && prune` must not run on a broken
    /// store. `--json` emits a machine-readable status object. The
    /// current doctor covers the WEB core's store only — the flag `--core
    /// web` makes that boundary explicit (all-core dispatch is future
    /// work; a doctor named wider than its evidence is a false claim).
    /// (Kiểm tra sức khỏe store (đọc được DB, an toàn root CAS) và báo
    /// các bước sửa — đích của mọi lời từ chối prune (P1-6 + P0-C).
    /// `--repair` khóa lại store bị cách ly và dọn temp sót. HỢP ĐỒNG
    /// EXIT (P0-B): 0 = khỏe (hoặc đã repair VÀ re-check khỏe), 1 =
    /// hỏng/vi phạm invariant — script và CI phải tin được exit code,
    /// `doctor && prune` không được chạy trên store hỏng. `--json` xuất
    /// đối tượng trạng thái máy đọc được. Doctor hiện tại chỉ phủ store
    /// của core WEB — cờ `--core web` ghi rõ biên đó (dispatch all-core
    /// là việc sau; doctor đặt tên rộng hơn bằng chứng là claim giả).)
    Doctor {
        #[arg(long, help = "attempt automatic repair, not just a check")]
        repair: bool,
        #[arg(
            long,
            default_value = "web",
            help = "store core to inspect (web is the only implemented doctor)"
        )]
        core: String,
        #[arg(long, help = "emit machine-readable JSON status")]
        json: bool,
    },
}

#[derive(Debug, serde::Serialize)]
pub struct PruneReport {
    pub unreferenced: Vec<String>,
    pub removed: Vec<String>,
    pub removed_bytes: u64,
    pub dry_run: bool,
}

fn store_db_for(project_root: &Path) -> Result<PathBuf> {
    let store_root = project_root.join(".magicore").join("cache").join("web");
    Ok(store_root.join("store.db"))
}

/// Locks directory for the doctor's GC (P2-1): the install lock lives
/// under the USER store root by default (Database::project_install_lock
/// → <store>/locks/), NOT under the project cache — the lock must be
/// shared by every process touching the same store DB, and the doctor's
/// project_root keys (lease.project_root) are store-layout roots.
/// Mirrors the install lock location: per-PROJECT locks under the web
/// cache layout (NOT the user-global store), so `acquire_at` probes the
/// SAME lock file the install actually takes — on every machine,
/// including locked-HOME environments.
/// (Thư mục locks cho GC của doctor: theo project, khớp install thật.)
fn locks_dir_for(project_root: &Path) -> std::path::PathBuf {
    project_root
        .join(".magicore")
        .join("cache")
        .join("web")
        .join("locks")
}

fn vstore_root_for(project_root: &Path) -> PathBuf {
    project_root.join("node_modules").join(".magicore")
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        stack.push(entry.path());
                    } else if let Ok(meta) = entry.metadata() {
                        total += meta.len();
                    }
                }
            }
        }
    }
    total
}

/// Copy toàn bộ cây thư mục (per-file — không dùng `cp` ngoài, portable).
/// Nguồn không tồn tại → trả Ok (no-op) để backup thiếu vstore vẫn chạy được.
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst).with_context(|| format!("create {}", dst.display()))?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir(&from, &to)?;
        } else if ft.is_file() || ft.is_symlink() {
            std::fs::copy(&from, &to)
                .with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Xoá nội dung thư mục đích (giữ chính thư mục) trước khi restore — fail nếu không xoá được.
fn clear_dir(dst: &Path) -> Result<()> {
    if !dst.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dst)? {
        let entry = entry?;
        let p = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&p).with_context(|| format!("remove {}", p.display()))?;
        } else {
            std::fs::remove_file(&p).with_context(|| format!("remove {}", p.display()))?;
        }
    }
    Ok(())
}

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

fn default_backup_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let base = Path::new(&home).join(".magicore");
    std::fs::create_dir_all(&base)?;
    Ok(base.join(format!("store_backup_{}", timestamp())))
}

fn prune_unreferenced(project_root: &Path, dry_run: bool) -> Result<PruneReport> {
    use mgc_store::database::Database;

    let db = Database::open(&store_db_for(project_root)?)?;
    let unreferenced = db.list_unreferenced()?;
    let mut report = PruneReport {
        unreferenced: unreferenced.iter().map(|id| id.to_string()).collect(),
        removed: Vec::new(),
        removed_bytes: 0,
        dry_run,
    };

    let vstore_root = vstore_root_for(project_root);
    for id in &unreferenced {
        let vstore_dir = vstore_root.join(format!(
            "{}@{}",
            id.name_str().replace('/', "+"),
            id.version()
        ));
        if vstore_dir.exists() {
            report.removed_bytes += dir_size(&vstore_dir);
            if !dry_run {
                std::fs::remove_dir_all(&vstore_dir)?;
            }
        }
        report.removed.push(id.to_string());
        if !dry_run {
            db.remove_package(id)?;
        }
    }
    Ok(report)
}

/// mgc store prune — delete unreferenced packages (02 §2.2).
pub async fn run(cmd: StoreCmd) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root =
        ProjectConfig::find_project_root(&cwd).ok_or_else(crate::error::project_root_missing)?;

    match cmd {
        StoreCmd::Prune { dry_run, json } => {
            let report = prune_unreferenced(&project_root, dry_run)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Store prune {}: {} unreferenced package(s)",
                    if dry_run { "(dry-run)" } else { "(pruned)" },
                    report.unreferenced.len()
                );
                for removed in &report.removed {
                    println!("  removed {removed}");
                }
                if report.removed_bytes > 0 {
                    println!("  freed {} bytes", report.removed_bytes);
                }
            }
            Ok(())
        }
        StoreCmd::Status => {
            let db = mgc_store::database::Database::open(&store_db_for(&project_root)?)?;
            let installed = db.list_installed()?;
            println!("Store status: {} installed package(s)", installed.len());
            println!(
                "virtual store: {}",
                vstore_root_for(&project_root).display()
            );
            Ok(())
        }
        StoreCmd::Backup { path } => {
            let backup_dir = match path {
                Some(p) => PathBuf::from(p),
                None => default_backup_dir()?,
            };
            // Backup 2 phần: store.db (index) + vstore (packages thật)
            let db_src = store_db_for(&project_root)?;
            let db_dst = backup_dir.join("store.db");
            let vstore_dst = backup_dir.join("vstore");
            if db_src.exists() {
                if let Some(parent) = db_dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&db_src, &db_dst)
                    .with_context(|| format!("copy {} → {}", db_src.display(), db_dst.display()))?;
            }
            copy_dir(&vstore_root_for(&project_root), &vstore_dst)?;
            println!("backup store → {}", backup_dir.display());
            println!(
                "  store.db: {} bytes",
                std::fs::metadata(&db_dst).map(|m| m.len()).unwrap_or(0)
            );
            println!("  vstore: {} bytes", dir_size(&vstore_dst));
            println!(
                "  restore: mgc store restore --path {}",
                backup_dir.display()
            );
            Ok(())
        }
        StoreCmd::Restore { path } => {
            let backup_dir = PathBuf::from(path);
            let db_src = backup_dir.join("store.db");
            let vstore_src = backup_dir.join("vstore");
            if !db_src.exists() && !vstore_src.exists() {
                bail!(
                    "invalid backup: {} (missing store.db and vstore)",
                    backup_dir.display()
                );
            }
            // Fail-closed: điểm yếu ghi đè — tự backup trước khi khôi phục
            let safety_backup = default_backup_dir()?;
            let db_dst = store_db_for(&project_root)?;
            copy_dir(
                &vstore_root_for(&project_root),
                &safety_backup.join("vstore"),
            )?;
            if db_dst.exists() {
                std::fs::copy(&db_dst, safety_backup.join("store.db"))?;
            }
            if db_src.exists() {
                if let Some(parent) = db_dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&db_src, &db_dst)?;
            }
            clear_dir(&vstore_root_for(&project_root))?;
            copy_dir(&vstore_src, &vstore_root_for(&project_root))?;
            println!("restore store ← {}", backup_dir.display());
            println!(
                "  safe backup (before restore): {}",
                safety_backup.display()
            );
            Ok(())
        }
        StoreCmd::Doctor { repair, core, json } => doctor(&project_root, repair, &core, json),
    }
}

/// Store health check + remediation (P1-6 + P0-C, Tech Lead vòng-7):
/// every fail-closed refusal (prune, install) points HERE. The doctor
/// verifies the two pillars the refcount system stands on:
///
/// 1. store.db opens, its schema parses, and live CAS refs are readable;
/// 2. the CAS root itself passes the same safety validation the store
///    applies at open (real dir, no symlink ancestry — P0-A/P0-B).
///
/// `--repair` additionally clears stale orphan temps under the CAS tmp/
/// (crash leftovers — safe to delete by age) and reports anything it
/// cannot fix. The doctor NEVER deletes blobs: without a readable
/// refcount, deletion is exactly what P0-C forbids.
///
/// Kiểm tra sức khỏe store + hướng sửa (P1-6 + P0-C): mọi lời từ chối
/// fail-closed (prune, install) trỏ VÀO ĐÂY. Doctor verify hai trụ cột
/// mà hệ refcount đứng trên:
///
/// 1. store.db mở được, schema parse được, CAS ref sống đọc được;
/// 2. root CAS tự nó qua cùng bước validate an toàn mà store áp lúc mở
///    (dir thật, không symlink ancestor — P0-A/P0-B).
///
/// `--repair` dọn thêm temp mồ côi stale dưới CAS tmp/ (tàn dư crash —
/// xóa theo tuổi là an toàn) và báo mọi thứ không sửa được. Doctor
/// KHÔNG BAO GIỜ xóa blob: không đọc được refcount thì xóa đúng là điều
/// P0-C cấm.
/// PID liveness probe (Gate 11-B lease GC): zero-signal `kill(pid, 0)`
/// — POSIX semantics, no signal delivered, only existence/permission
/// checked. Returns None where the probe is unavailable (non-unix) —
/// the caller then falls back to the grace window alone (conservative).
/// (Thăm dò pid còn sống: `kill(pid, 0)` zero-signal — semantics POSIX,
/// không gửi tín hiệu, chỉ kiểm tra tồn tại/quyền. Trả None nơi thăm dò
/// không khả dụng (non-unix) — caller rơi về cửa sổ xử lý thuần (bảo
/// toàn).)
/// SAFETY PROOF: `kill(pid, 0)` is the documented POSIX liveness probe
/// — signal 0 delivers NOTHING to the target, the kernel only checks
/// process existence and permission. No memory is touched, no
/// invariants to uphold; the single argument is a plain integer.
/// (Giải trình an toàn: `kill(pid, 0)` là thăm dò tồn tại POSIX có tài
/// liệu — signal 0 KHÔNG gửi gì tới đích, kernel chỉ kiểm tra process
/// tồn tại và quyền. Không chạm bộ nhớ, không invariant phải giữ; đối
/// số duy nhất là số nguyên thuần.)
#[cfg(unix)]
#[allow(unsafe_code)]
fn pid_alive_on_this_os(pid: u32) -> Option<bool> {
    let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
    match ret {
        0 => Some(true),
        -1 => {
            // ESRCH = no such process (dead); EPERM = exists but owned by
            // someone else (ALIVE).
            // (ESRCH = không có process (chết); EPERM = có nhưng thuộc
            // user khác (CÒN SỐNG).)
            Some(std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH))
        }
        _ => Some(false),
    }
}

#[cfg(not(unix))]
fn pid_alive_on_this_os(_pid: u32) -> Option<bool> {
    None
}

/// Default staging-lease grace window (seconds): a staging lease older
/// than this whose pid is unverifiable is garbage. Overridable for tests
/// via `MGC_DOCTOR_STAGING_GRACE_SECS` (RULE §12 — no inline magic).
/// (Cửa sổ xử lý lease staging mặc định (giây): lease staging cũ hơn mà
/// pid không kiểm chứng được là rác. Override cho test qua
/// `MGC_DOCTOR_STAGING_GRACE_SECS` (RULE §12 — không literal ẩn).)
const DEFAULT_STAGING_GRACE_SECS: i64 = 24 * 60 * 60;

/// Read the staging grace window from the env override or the default.
/// (Đọc cửa sổ xử lý từ env override hoặc mặc định.)
fn staging_grace_secs() -> i64 {
    std::env::var("MGC_DOCTOR_STAGING_GRACE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_STAGING_GRACE_SECS)
}

/// Staging-lease disposition under the Gate 11-B policy — the ONE source
/// of truth for both the doctor's health verdict and the --repair GC.
/// (Phân loại lease staging theo chính sách Gate 11-B — nguồn sự thật DUY
/// NHẤT cho cả verdict sức khỏe doctor lẫn GC --repair.)
#[derive(Debug, PartialEq, Eq)]
enum StagingDisposition {
    /// Live: pid alive AND lease fresh, OR the project install lock is
    /// held — an in-flight install must never be counted stale.
    /// (Live: pid sống VÀ lease mới, HOẶC install lock project bị giữ —
    /// install đang bay không bao giờ bị tính stale.)
    Live,
    /// Stale: lease dead (pid gone / lease ancient) with zero claims
    /// (leaked begin) or claims all covered by a promoted generation
    /// (redundant) — retirable garbage that FAILS health until repaired.
    /// (Stale: lease chết (pid mất / lease cổ) không claim (begin rò) hoặc
    /// claim đều được promoted generation giữ (thừa) — rác nghỉ hưu được,
    /// làm hỏng health cho tới khi repair.)
    Stale,
    /// Retained: lease dead but at least one claimed hash has no promoted
    /// generation vouching for it — the claim is that blob's ONLY
    /// protection, so over-retention is SAFE and must NOT fail health.
    /// (Retained: lease chết nhưng có hash chưa được promoted nào giữ —
    /// claim là bảo vệ DUY NHẤT của blob, giữ thừa AN TOÀN, KHÔNG hỏng.)
    Retained,
}

/// Result of classifying every staging lease in a store.
struct StagingClassification {
    live: usize,
    stale: Vec<mgc_store::StagingLease>,
    retained: usize,
}

/// Classify every staging lease of `db` with the SHARED policy: pid
/// liveness (Unix) + lease age + project-install-lock + promoted coverage.
/// Both the doctor's count path and the --repair GC call this — never two
/// diverged copies of the same rules.
/// (Phân loại mọi lease staging của db bằng chính sách DÙNG CHUNG: pid
/// sống (Unix) + tuổi lease + install-lock project + độ phủ promoted. Cả
/// path đếm của doctor lẫn GC --repair đều gọi đây — không 2 bản copy lệch.)
fn classify_staging_leases(
    db: &mgc_store::Database,
    locks_root: &Path,
    grace_secs: i64,
) -> Result<StagingClassification> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let leases = db.list_staging_leases()?;
    let covered: std::collections::HashSet<(String, i64)> = db
        .list_staging_leases_covered_by_promoted()?
        .into_iter()
        .map(|l| (l.project_root, l.generation))
        .collect();
    let mut live = 0usize;
    let mut stale = Vec::new();
    let mut retained = 0usize;
    for lease in leases {
        match classify_staging_lease(&lease, &covered, locks_root, now_secs, grace_secs) {
            StagingDisposition::Live => live += 1,
            StagingDisposition::Stale => stale.push(lease),
            StagingDisposition::Retained => retained += 1,
        }
    }
    Ok(StagingClassification {
        live,
        stale,
        retained,
    })
}

/// Classify ONE lease (pure decision logic shared by count + repair).
/// (Phân loại MỘT lease (logic quyết định thuần, dùng chung count + repair).)
fn classify_staging_lease(
    lease: &mgc_store::StagingLease,
    covered: &std::collections::HashSet<(String, i64)>,
    locks_root: &Path,
    now_secs: i64,
    grace_secs: i64,
) -> StagingDisposition {
    let pid_alive = lease
        .lease_pid
        .and_then(|pid| pid_alive_on_this_os(pid as u32))
        .unwrap_or(false);
    let lease_fresh = lease
        .lease_started_at
        .is_some_and(|started| now_secs - started < grace_secs);
    // P2-1: the project install lock is the cross-platform liveness proof
    // (the pid probe is Unix-only). Holding it ⇒ live, regardless of pid.
    // (P2-1: install lock project là bằng chứng sống cross-platform (pid
    // probe chỉ Unix). Giữ nó ⇒ live, bất kể pid.)
    let lock_held =
        mgc_store::ProjectInstallLock::acquire_at(locks_root, &lease.project_root).is_err();
    let covered_by_promoted = covered.contains(&(lease.project_root.clone(), lease.generation));
    classify_staging_lease_pure(
        lease.claim_count,
        pid_alive,
        lease_fresh,
        lock_held,
        covered_by_promoted,
    )
}

/// Pure decision core of the staging-lease classifier (Gate 11-B.2): the
/// same LIVE / STALE / RETAINED rules WITHOUT the pid or lock probes, so the
/// disposition logic is directly unit-testable. `classify_staging_lease`
/// computes the probes and delegates here — never two copies of the rules.
/// (Lõi quyết định thuần của classifier lease staging (Gate 11-B.2): cùng
/// luật LIVE / STALE / RETAINED NHƯNG không có probe pid hay lock, để logic
/// disposition test đơn vị trực tiếp được. `classify_staging_lease` tính
/// probe rồi ủy quyền vào đây — không bao giờ hai bản copy lệch nhau.)
fn classify_staging_lease_pure(
    claim_count: i64,
    pid_alive: bool,
    lease_fresh: bool,
    lock_held: bool,
    covered_by_promoted: bool,
) -> StagingDisposition {
    if (pid_alive && lease_fresh) || lock_held {
        return StagingDisposition::Live;
    }
    if claim_count == 0 || covered_by_promoted {
        StagingDisposition::Stale
    } else {
        StagingDisposition::Retained
    }
}

fn doctor(project_root: &Path, repair: bool, core: &str, json: bool) -> Result<()> {
    // Scope gate (P0-D taxonomy, vòng-9): only the WEB core store has a
    // doctor today — refuse anything else loudly instead of silently
    // checking web while the user asked for cargo/pypi.
    // (Cổng phạm vi: chỉ store core WEB có doctor hôm nay — từ chối thứ
    // khác rõ ràng thay vì âm thầm check web khi user hỏi cargo/pypi.)
    if core != "web" {
        return Err(anyhow::anyhow!(
            "store doctor currently supports only --core web; \
             other core stores are future work"
        ));
    }
    let web_cache = project_root.join(".magicore").join("cache").join("web");
    let db_path = store_db_for(project_root)?;
    let cas_root = web_cache.join("cas");
    let tmp_dir = web_cache.join("tmp");
    // Stale-staging GC needs the grace window + lock root in BOTH the
    // count path and the repair path — compute once, share one value.
    // (GC stale-staging cần cửa sổ xử lý + root lock ở CẢ path đếm lẫn
    // path repair — tính một lần, dùng chung một giá trị.)
    let locks_root = locks_dir_for(project_root);
    let grace_secs = staging_grace_secs();
    // Hard failures (unreadable DB, orphan/multi-promoted/missing/corrupt,
    // unsafe CAS root/tmp) are NOT repairable by the stale GC. Stale
    // staging is tracked in `report` and may be retired by --repair, so it
    // is folded into the FINAL verdict below, not this flag.
    // (Lỗi cứng (DB không đọc được, orphan/multi-promoted/missing/corrupt,
    // CAS root/tmp không an toàn) KHÔNG sửa được bằng GC stale. Staging
    // stale theo dõi trong `report` và --repair nghỉ hưu được, nên gộp vào
    // verdict CUỐI bên dưới, không phải flag này.)
    let mut hard_failure = false;
    // Promoted-uniqueness count — hoisted so the final recheck can read it.
    // (Số promoted trùng — nâng lên ngoài để lần recheck cuối đọc được.)
    let mut multi_promoted: i64 = 0;

    // Structured result for --json (P0-B): stable machine-readable
    // contract, one object, fields stable across releases. Generation
    // health fields (Gate 11-B, vòng-11): the doctor now proves token
    // invariants, not just "SQLite opens" — orphan claims (no marker),
    // stale staging generations (crashed installs), promoted uniqueness
    // per project, and referenced-blob existence.
    // (Kết quả có cấu trúc cho --json (P0-B): hợp đồng máy đọc ổn định,
    // một object, field ổn định qua các release. Field sức khỏe
    // generation (Gate 11-B): doctor giờ chứng minh invariant token,
    // không chỉ "SQLite mở được" — claim mồ côi (thiếu marker), staging
    // generation chết (install đứt), tính duy nhất promoted mỗi project,
    // và sự tồn tại của blob được tham chiếu.)
    #[derive(serde::Serialize)]
    struct DoctorReport {
        core: String,
        store_root: String,
        db_ok: bool,
        cas_root_ok: bool,
        tmp_dir_ok: bool,
        repaired: bool,
        healthy: bool,
        generation_state_ok: bool,
        orphan_claims: usize,
        stale_staging_generations: usize,
        retained_staging_generations: usize,
        missing_referenced_blobs: usize,
        corrupt_referenced_blobs: usize,
    }
    let mut report = DoctorReport {
        core: core.to_string(),
        store_root: web_cache.display().to_string(),
        db_ok: false,
        cas_root_ok: false,
        tmp_dir_ok: true,
        repaired: false,
        healthy: false,
        generation_state_ok: false,
        orphan_claims: 0,
        stale_staging_generations: 0,
        retained_staging_generations: 0,
        missing_referenced_blobs: 0,
        corrupt_referenced_blobs: 0,
    };

    if !json {
        println!("store doctor: {}", web_cache.display());
    }

    // Pillar 1 — DB readable + refset loadable + GENERATION INVARIANTS
    // (Gate 11-B, vòng-11): the old doctor proved only "SQLite opens".
    // Store integrity needs the token invariants too:
    //   (a) no claim without a generation marker (orphan);
    //   (b) no stale STAGING generation older than a grace window
    //       (crashed installs — over-retention is safe but must be
    //       VISIBLE, and --repair may retire them);
    //   (c) at most one PROMOTED generation per project (uniqueness);
    //   (d) every referenced blob EXISTS on disk and rehashes to its
    //       digest (missing/corrupt).
    // (Trụ 1 — DB đọc được + refset load được + INVARIANT GENERATION:
    // doctor cũ chỉ chứng minh "SQLite mở được". Sức khỏe store cần cả
    // invariant token: (a) không claim thiếu marker (mồ côi); (b) không
    // staging generation chết quá cửa sổ xử lý (install đứt — giữ thừa
    // an toàn nhưng phải HIỆN, --repair được nghỉ hưu); (c) tối đa MỘT
    // generation promoted mỗi project; (d) mọi blob được tham chiếu
    // TỒN TẠI và rehash đúng digest (mất/hỏng).)
    if db_path.exists() {
        match mgc_store::Database::open(&db_path)
            .and_then(|db| db.list_cas_live_refs().map(|live| (db, live)))
        {
            Ok((db, live)) => {
                report.db_ok = true;
                if !json {
                    println!("  [ok] store.db readable — {} live CAS ref(s)", live.len());
                }
                // (c) promoted uniqueness via SQL + (b) stale/retained via
                // the shared lease classifier (Gate 11-B P0-1). The old SQL
                // counted "stale" as claim-less staging ONLY — it ignored
                // lease/pid/lock entirely and never flagged a claim-ful
                // crashed staging whose hashes are all promoted-covered
                // (the exact leak the P2-2 GC retires). The classifier now
                // splits LIVE / STALE / RETAINED with one policy.
                // ((c) duy nhất promoted qua SQL + (b) stale/retained qua
                // classifier lease dùng chung (P0-1). SQL cũ tính "stale"
                // chỉ theo staging KHÔNG claim — bỏ qua lease/pid/lock và
                // không bao giờ cờ staging đứt CÓ claim mà hash đều được
                // promoted giữ (đúng chỗ rò mà GC P2-2 nghỉ hưu). Classifier
                // giờ tách LIVE / STALE / RETAINED bằng một chính sách.)
                multi_promoted = {
                    let conn = db.conn();
                    let mut stmt = conn
                        .prepare(
                            "SELECT COALESCE(SUM(
                                 CASE WHEN state = 'promoted' AND (
                                     SELECT COUNT(*) FROM cas_generations g4
                                     WHERE g4.project_root = cas_generations.project_root
                                       AND g4.state = 'promoted'
                                 ) > 1 THEN 1 ELSE 0 END
                             ), 0)
                             FROM cas_generations",
                        )
                        .map_err(|e| anyhow::anyhow!("promoted uniqueness query failed: {e}"))?;
                    stmt.query_row([], |row| row.get::<_, i64>(0))
                        .map_err(|e| anyhow::anyhow!("promoted uniqueness scan failed: {e}"))?
                };
                let classification = classify_staging_leases(&db, &locks_root, grace_secs)?;
                report.stale_staging_generations = classification.stale.len();
                report.retained_staging_generations = classification.retained;
                // (a) orphan claims: rows whose (project, generation) has
                // no marker — impossible with FK on, a violation means
                // hand-edited DB.
                // ((a) claim mồ côi: row mà (project, generation) thiếu
                // marker — bất khả khi FK bật, vi phạm nghĩa là DB bị sửa tay.)
                let orphan_claims: i64 = {
                    let conn = db.conn();
                    let mut stmt = conn
                        .prepare(
                            "SELECT COUNT(*) FROM cas_blob_refs r
                             WHERE NOT EXISTS (
                                 SELECT 1 FROM cas_generations g
                                 WHERE g.project_root = r.project_root
                                   AND g.generation = r.generation
                             )",
                        )
                        .map_err(|e| anyhow::anyhow!("orphan claim query failed: {e}"))?;
                    stmt.query_row([], |row| row.get(0))
                        .map_err(|e| anyhow::anyhow!("orphan claim scan failed: {e}"))?
                };
                report.orphan_claims = orphan_claims as usize;
                // (d) referenced blobs exist + rehash. Claims store the bare
                // hex; the blob lives at cas/files/blake3/<xx>/<hash> (+
                // `.exec` variant — either counts as present).
                // ((d) blob được tham chiếu tồn tại + rehash. Claim lưu hex
                // thuần; blob nằm ở cas/files/blake3/<xx>/<hash> (biến thể
                // `.exec` — một trong hai là có).)
                let mut missing = 0usize;
                let mut corrupt = 0usize;
                if !live.is_empty() && cas_root.exists() {
                    for hash in &live {
                        let blob = cas_root
                            .join("files")
                            .join("blake3")
                            .join(&hash[..2])
                            .join(hash);
                        let exec_blob = {
                            let mut p = blob.clone().into_os_string();
                            p.push(".exec");
                            std::path::PathBuf::from(p)
                        };
                        let path = if blob.exists() {
                            blob
                        } else if exec_blob.exists() {
                            exec_blob
                        } else {
                            missing += 1;
                            continue;
                        };
                        // Rehash verify — doctor's whole point (trust the
                        // refset, so verify the blob it vouches for).
                        // P3 (fresh-context review 2026-09-15): STREAMING
                        // hash — `fs::read` loaded the whole blob into
                        // RAM; a store with AI-model/game-asset blobs
                        // made the doctor's RSS balloon. hash_file
                        // streams an 8-KiB buffer; same BLAKE3 digest.
                        // (Rehash verify — điểm cốt lõi của doctor (tin
                        // refset thì phải verify blob nó bảo chứng).
                        // P3: hash STREAMING — `fs::read` load cả blob
                        // vào RAM; store có blob cỡ AI-model/game-asset
                        // làm RSS của doctor phồng. hash_file stream
                        // buffer 8-KiB; cùng digest BLAKE3.)
                        match mgc_crypto::Blake3Hasher::hash_file(&path) {
                            Ok(digest) => {
                                if digest.to_hex() != *hash {
                                    corrupt += 1;
                                }
                            }
                            Err(_) => missing += 1,
                        }
                    }
                }
                report.missing_referenced_blobs = missing;
                report.corrupt_referenced_blobs = corrupt;
                // Pre-repair verdict (Gate 11-B P0-1): stale staging now
                // FAILS health — a leaked begin / redundant claim is garbage
                // that must be retired, not silently tolerated. Retained is
                // SAFE over-retention and stays out of the verdict.
                // (Verdict trước-repair (P0-1): staging stale giờ LÀM HỎNG
                // health — begin rò / claim thừa là rác phải nghỉ hưu, không
                // dung thứ im lặng. Retained là giữ thừa AN TOÀN, nằm ngoài
                // verdict.)
                let generation_ok = report.orphan_claims == 0
                    && multi_promoted == 0
                    && missing == 0
                    && corrupt == 0
                    && report.stale_staging_generations == 0;
                report.generation_state_ok = generation_ok;
                if multi_promoted != 0 || report.orphan_claims != 0 || missing != 0 || corrupt != 0
                {
                    hard_failure = true;
                }
                if !json {
                    println!(
                        "  [{}] generation invariants: {} orphan claim(s), {} stale staging gen(s), {} missing blob(s), {} corrupt blob(s)",
                        if generation_ok { "ok" } else { "FAIL" },
                        report.orphan_claims,
                        report.stale_staging_generations,
                        report.missing_referenced_blobs,
                        report.corrupt_referenced_blobs
                    );
                    if report.retained_staging_generations > 0 {
                        println!(
                            "  [ok] {} retained staging gen(s) (safe over-retention — not corruption)",
                            report.retained_staging_generations
                        );
                    }
                    if report.stale_staging_generations > 0 {
                        println!(
                            "  repair hint: run mgc store doctor --repair to retire leaked staging generation(s)"
                        );
                    }
                }
            }
            Err(e) => {
                hard_failure = true;
                if !json {
                    println!("  [FAIL] store.db unreadable: {e}");
                    println!(
                        "  repair hint: restore store.db from a backup \
                         (mgc store backup/restore), or move the corrupted db \
                         aside and re-install projects to rebuild claims"
                    );
                }
            }
        }
    } else {
        hard_failure = true;
        if !json {
            println!("  [FAIL] store.db missing at {}", db_path.display());
            println!(
                "  repair hint: re-run mgc install in the project to rebuild \
                 the refset (prune stays disabled until then)"
            );
        }
    }

    // Pillar 2 — CAS root safety (P0-A/P0-B: symlink checks must hold at
    // doctor time too — a root swapped to a symlink between installs is
    // a live attack window, not a stale artifact).
    // (Trụ 2 — an toàn root CAS (P0-A/P0-B: check symlink phải còn đúng
    // lúc doctor — root bị đổi thành symlink giữa 2 lần install là cửa
    // sổ tấn công sống, không phải tàn dư cũ).)
    if cas_root.exists() {
        match mgc_store::cas::validate_cas_root(&cas_root) {
            Ok(()) => {
                report.cas_root_ok = true;
                if !json {
                    println!("  [ok] CAS root is a real directory (no symlink ancestry)");
                }
            }
            Err(e) => {
                hard_failure = true;
                if !json {
                    println!("  [FAIL] CAS root unsafe: {e}");
                    println!(
                        "  repair hint: move the store to a real directory and \
                         remove the symlink chain — refusing to operate through \
                         a redirected root"
                    );
                }
            }
        }
    } else {
        report.cas_root_ok = true;
        if !json {
            println!("  [ok] no CAS directory yet (nothing to validate)");
        }
    }

    // Orphan temp GC (--repair, P0-C HARDENING vòng-9): the old code took
    // `tmp_dir` from `cas_root.parent()` — correct path, but it derefed
    // entries via `entry.metadata()` (follows symlinks) and would happily
    // recurse through a symlinked `tmp` into files OUTSIDE the store. The
    // GC now (1) refuses to run through a symlinked tmp (no-follow
    // `symlink_metadata` on the dir itself), (2) skips every entry whose
    // own metadata is a symlink — never follows into a redirect, (3) only
    // removes names carrying the MGC temp-name schema
    // (`<prefix>-<pid>-<tid>-<nanos>-<counter>`), leaving foreign files
    // alone, and (4) reports — never guesses — when the tmp dir itself is
    // unsafe.
    // (GC temp mồ côi (--repair, cứng hóa P0-C vòng-9): code cũ lấy
    // `tmp_dir` từ `cas_root.parent()` — path đúng, nhưng deref entry qua
    // `entry.metadata()` (follow symlink) và sẵn sàng đệ quy qua `tmp`
    // là symlink vào file NGOÀI store. GC giờ (1) từ chối chạy qua tmp
    // là symlink (no-follow `symlink_metadata` trên chính dir), (2) bỏ
    // qua mọi entry mà metadata của nó là symlink — không đi theo
    // redirect, (3) chỉ xóa tên theo schema temp của MGC
    // (`<prefix>-<pid>-<tid>-<nanos>-<counter>`), file ngoài để nguyên,
    // (4) báo — không đoán — khi chính tmp dir không an toàn.)
    let tmp_meta_ok = std::fs::symlink_metadata(&tmp_dir)
        .map(|m| m.is_dir() && !m.file_type().is_symlink())
        .unwrap_or(false);
    if repair && tmp_dir.exists() {
        if !tmp_meta_ok {
            hard_failure = true;
            report.tmp_dir_ok = false;
            if !json {
                println!(
                    "  [FAIL] store tmp is not a real directory ({}): refusing \
                     to sweep through a redirect",
                    tmp_dir.display()
                );
                println!(
                    "  repair hint: replace the tmp entry with a real \
                     directory, then re-run mgc store doctor --repair"
                );
            }
        } else {
            let day = std::time::Duration::from_secs(24 * 60 * 60);
            let mut removed = 0usize;
            let mut skipped_foreign = 0usize;
            for entry in std::fs::read_dir(&tmp_dir)? {
                let entry = entry?;
                // No-follow: the ENTRY's own metadata decides — a symlinked
                // child is skipped, never traversed.
                // (No-follow: metadata của chính ENTRY quyết định — con là
                // symlink bị bỏ qua, không đệ quy.)
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.file_type().is_symlink() {
                    skipped_foreign += 1;
                    continue;
                }
                // SHARED PARSER (Gate 11-B / P0-7, vòng-11): the old loose
                // heuristic (`has '-', ≥4 parts, digits tail`) matched
                // foreign dashed names like `customer-important-backup-123`
                // and DELETED them. Now the doctor uses the SAME strict
                // parser as `unique_tmp_path` (prefix allowlist + exact
                // `<pid>-<tid_hex>-<nanos>-<counter>` arity) and additionally
                // requires the name be sweepable in THIS directory
                // (`import-file`/`write-bytes` — the only prefixes that
                // legally stage under CAS tmp). Everything else is foreign
                // and untouched.
                // (PARSER DÙNG CHUNG (P0-7): heuristic lỏng cũ (`có '-',
                // ≥4 phần, đuôi số) khớp cả tên ngoài như
                // `customer-important-backup-123` và XÓA nó. Giờ doctor
                // dùng parser NGHIÊM NGẶT giống `unique_tmp_path` (allowlist
                // prefix + đúng arity `<pid>-<tid_hex>-<nanos>-<counter>`)
                // và thêm điều kiện tên được quét ở THƯ MỤC NÀY
                // (`import-file`/`write-bytes` — prefix duy nhất hợp pháp
                // staging dưới tmp CAS). Mọi tên khác là của ngoài, không đụng.)
                let file_name = entry.file_name();
                let Some(name) = file_name.to_str() else {
                    skipped_foreign += 1;
                    continue;
                };
                let sweepable = mgc_store::cas::TempFileName::parse(name)
                    .is_some_and(|parsed| parsed.cas_tmp_sweepable());
                if !sweepable {
                    skipped_foreign += 1;
                    continue;
                }
                let stale = meta
                    .modified()
                    .ok()
                    .and_then(|m| m.elapsed().ok())
                    .is_some_and(|age| age > day);
                if stale {
                    let p = entry.path();
                    let removed_ok = if meta.is_dir() {
                        std::fs::remove_dir_all(&p)
                    } else {
                        std::fs::remove_file(&p)
                    };
                    if removed_ok.is_ok() {
                        removed += 1;
                    }
                }
            }
            report.repaired = removed > 0;
            if !json {
                println!("  [ok] orphan-temp GC removed {removed} stale file(s)");
                if skipped_foreign > 0 {
                    println!(
                        "  [ok] skipped {skipped_foreign} non-MGC-named or \
                         symlinked entr(ies) — never swept"
                    );
                }
            }
        }
    }

    // Stale-staging repair (Gate 11-B, vòng-11 — lease-aware): the OLD
    // GC only retired claim-LESS staging markers; the verdict demands
    // lease/pid awareness. Contract:
    //   (a) claim-less staging + lease pid DEAD (or ancient beyond
    //       grace) → garbage → --repair aborts it;
    //   (b) claim-less staging + pid ALIVE → a begin in flight — NEVER
    //       touched (report only);
    //   (c) claim-ful staging → either a live install or a crashed one
    //       still over-protecting blobs — NEVER touched here (its own
    //       abort / a future GC with a full claim-map retires it).
    // pid liveness: kill(pid, 0) semantics via the `sysinfo`-free route —
    // a zero-signal probe. On non-unix the probe is unavailable → the
    // doctor falls back to the grace window alone (still conservative).
    // (Sửa staging chết — có tri lease: GC cũ chỉ nghỉ hưu staging
    // KHÔNG claim; phán quyết đòi nhận thức lease/pid. Hợp đồng:
    // (a) staging không claim + pid lease CHẾT (hoặc quá cũ quá cửa sổ) →
    // rác → --repair hủy; (b) staging không claim + pid CÒN SỐNG →
    // begin đang bay — KHÔNG BAO GIỜ đụng (chỉ báo); (c) staging CÓ
    // claim → hoặc install đang chạy hoặc install đứt vẫn bảo vệ blob
    // — KHÔNG BAO GIỜ đụng ở đây (abort của chính nó / GC tương lai với
    // claim-map đầy đủ mới nghỉ hưu).)
    //
    // P2-1 (fresh-context review 2026-09-15): the GC now takes the
    // project's INSTALL LOCK before aborting a staging generation of
    // that project. Rationale: the pid probe (kill(pid,0)) is Unix-only —
    // on Windows `pid_alive_on_this_os` returns None→false, so a
    // CLAIM-LESS IN-FLIGHT begin (lease fresh, claims not yet filed)
    // could be aborted MID-INSTALL by --repair; the install's next
    // cas_claim would then die with UnknownToken. The install lock is
    // the serialization boundary that closes the hole cross-platform:
    // an install holding the lock OWNS its staging generation; the
    // doctor can only retire staging of installs that are provably NOT
    // running (no lock held).
    // (P2-1: GC giờ lấy INSTALL LOCK của project trước khi hủy staging
    // generation của project đó. Lý do: pid probe chỉ có trên Unix —
    // trên Windows probe trả None→false, nên begin ĐANG CHẠY không claim
    // (lease mới, chưa ghi claim) có thể bị --repair hủy GIỮA CHỪNG;
    // cas_claim kế tiếp của install chết với UnknownToken. Install lock
    // là ranh giới xếp tuần tự đóng lỗ hổng cross-platform: install giữ
    // lock SỞ HỮU staging generation của nó; doctor chỉ nghỉ hưu staging
    // của install chứng minh được là KHÔNG chạy (không giữ lock).)
    //
    // P2-2 (fresh-context review 2026-09-15): claim-ful staging of a
    // DEAD install was NEVER retired before — it over-protected blobs
    // forever (safe, unbounded). The doctor now also retires the
    // evidence-backed subset via list_staging_leases_covered_by_promoted:
    // a claim-ful staging whose EVERY claimed hash is still claimed by a
    // PROMOTED generation of the same project loses nothing by being
    // aborted (the promoted refset subsumes it). Claim-ful staging with
    // hashes NOT covered by any promoted generation stays untouched —
    // those blobs would lose their only protection.
    // (P2-2: staging CÓ claim của install CHẾT chưa bao giờ được nghỉ
    // hưu — nó bảo vệ blob vô hạn (an toàn, không giới hạn). Doctor giờ
    // còn nghỉ hưu tập có bằng chứng qua list_staging_leases_covered_
    // by_promoted: staging có claim mà MỌI hash nó claim vẫn được
    // generation PROMOTED của cùng project claim thì hủy đi không mất
    // gì (refset promoted thâu hẹp nó). Staging có claim mà hash KHÔNG
    // được promoted nào giữ thì không đụng — blob đó mất bảo vệ duy
    // nhất.)
    if repair && report.db_ok {
        let db = mgc_store::Database::open(&db_path)?;
        // Shared classifier (single source of truth) — the SAME policy the
        // count path used above. Retire exactly the STALE subset; Live and
        // Retained are left alone (an in-flight install / a blob's only
        // protection must never be aborted).
        // (Classifier dùng chung (nguồn sự thật duy nhất) — CÙNG chính
        // sách path đếm dùng ở trên. Nghỉ hưu đúng tập STALE; Live và
        // Retained để nguyên (install đang bay / bảo vệ duy nhất của blob
        // không bao giờ bị hủy).)
        let classification = classify_staging_leases(&db, &locks_root, grace_secs)?;
        let mut retired = 0usize;
        for lease in &classification.stale {
            // TOCTOU: abort_cas_generation is token-gated and atomic on the
            // OLD generation number; a new install always allocates a NEW
            // token, so retiring a STALE generation is safe even if another
            // install started between classify and abort. The lock gate in
            // the classifier already guarantees we never touch a LIVE one.
            // (TOCTOU: abort_cas_generation có cổng token và nguyên tử trên
            // số generation CŨ; install mới luôn cấp token MỚI, nên nghỉ
            // hưu generation STALE an toàn kể cả khi install khác bắt đầu
            // giữa classify và abort. Cổng lock trong classifier đã bảo
            // đảm không bao giờ đụng generation LIVE.)
            if db
                .abort_cas_generation(&lease.project_root, lease.generation)
                .is_ok()
            {
                retired += 1;
            }
        }
        report.repaired = report.repaired || retired > 0;
        // Post-repair stale ledger: the invariants line above printed the
        // PRE-repair evidence (what the doctor FOUND); this field documents
        // what REMAINS, so the --json consumer and the final verdict read
        // the post-repair truth.
        // (Sổ stale sau-repair: dòng invariants phía trên in bằng chứng
        // TRƯỚC-repair (doctor TÌM THẤY gì); field này ghi còn LẠI gì, để
        // consumer --json và verdict cuối đọc sự thật sau-repair.)
        report.stale_staging_generations = report.stale_staging_generations.saturating_sub(retired);
        if !json {
            println!(
                "  [ok] lease GC retired {retired} leaked staging generation(s) \
                 (skipped: {} live, {} retained over-retention)",
                classification.live, classification.retained,
            );
        }
    }

    // Final verdict (P0-B + Gate 11-B P0-1): healthy iff no hard failure
    // (unreadable DB, orphan/multi-promoted/missing/corrupt, unsafe CAS
    // root/tmp) AND no stale staging remains (repair may have retired it).
    // Retained staging is SAFE over-retention and does not fail health.
    // (Verdict cuối (P0-B + P0-1): khỏe iff không lỗi cứng (DB không đọc
    // được, orphan/multi-promoted/missing/corrupt, CAS root/tmp không an
    // toàn) VÀ không còn staging stale (repair có thể đã nghỉ hưu). Staging
    // retained là giữ thừa AN TOÀN, không làm hỏng health.)
    let generation_ok = report.orphan_claims == 0
        && multi_promoted == 0
        && report.missing_referenced_blobs == 0
        && report.corrupt_referenced_blobs == 0
        && report.stale_staging_generations == 0;
    report.generation_state_ok = generation_ok;
    report.healthy = !hard_failure && generation_ok;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "store doctor: {}",
            if report.healthy {
                "HEALTHY"
            } else {
                "NEEDS REPAIR — prune/install refuse until fixed"
            }
        );
    }

    // EXIT CONTRACT (P0-B): healthy → Ok; broken → anyhow error so the CLI
    // exits 1 and `mgc store doctor && <next>` gates correctly. A broken
    // store must never read as success.
    // (Hợp đồng exit (P0-B): khỏe → Ok; hỏng → error anyhow để CLI exit 1
    // và `mgc store doctor && <bước sau>` chặn đúng. Store hỏng không
    // bao giờ được đọc là thành công.)
    if report.healthy {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "store doctor: NEEDS REPAIR — prune/install refuse until fixed \
             (re-run mgc store doctor for details)"
        ))
    }
}

// Test module — mounted from `src/test/store_test.rs` (RULE §5: no inline
// `mod tests` in `src/*.rs`; the child module keeps private-item access).
// (Module test — mount từ `src/test/store_test.rs` (RULE §5: không inline
// `mod tests` trong `src/*.rs`; module con giữ quyền truy cập private item).)
#[cfg(test)]
#[path = "../test/store_test.rs"]
mod tests;
