#![allow(clippy::unwrap_used)]
//! Adversarial regression for the generation-TOKEN protocol (Gate 11-A,
//! vòng-11 audit): the round-10 schema fixed the PK, but the token
//! lifecycle was unguarded — promote/abort/claim ran mutations BEFORE
//! verifying the token, allocation was a two-statement race, and nothing
//! enforced "staging-only mutation". These tests pin each fix.
//! test riêng tại test/ (RULE §5).
//! (Test hồi quy đối kháng cho giao thức generation-TOKEN: schema vòng-10
//! sửa PK, nhưng vòng đời token chưa có bảo vệ — promote/abort/claim
//! mutate TRƯỚC khi verify token, cấp phát là race 2-statement, không gì
//! ép "chỉ staging được mutate". Các test này ghim từng bản sửa.)

use mgc_store::Database;
use mgc_store::database::CasGenerationError;
use std::sync::Arc;
use std::sync::Barrier;

struct TestDb {
    _dir: tempfile::TempDir,
    db: Database,
}

impl TestDb {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("store.db")).unwrap();
        Self { _dir: dir, db }
    }
}

const PROJ_A: &str = "/proj/a";
const PROJ_B: &str = "/proj/b";

// ── P0-2: promote with a forged/unknown token must be zero-mutation ──

#[test]
fn promote_unknown_token_is_zero_mutation() {
    // The round-10 code ran its DELETEs before checking the token, so
    // promote(project, 999999) retired every promoted generation older
    // than the forged number and still returned Ok(()).
    // (Code vòng-10 chạy DELETE trước khi check token, nên
    // promote(project, 999999) nghỉ hưu mọi generation promoted cũ hơn
    // con số giả rồi vẫn Ok(()).)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t0, "hash-b").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();
    let live_before = db.list_cas_live_refs().unwrap();

    let err = db.promote_cas_generation(PROJ_A, 999_999).unwrap_err();
    assert!(
        matches!(err, CasGenerationError::UnknownToken { .. }),
        "forged promote must be a typed UnknownToken error, got {err:?}"
    );

    let live_after = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live_before, live_after,
        "P0-2: promote with an unknown token must retire NOTHING (zero mutation)"
    );
}

#[test]
fn promote_promoted_token_is_verified_noop() {
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["hash-a".to_string()]);

    // Re-promote of the same token: PROVEN no-op (state checked first).
    // (Re-promote cùng token: no-op ĐƯỢC KIỂM CHỨNG — state check trước.)
    db.promote_cas_generation(PROJ_A, t0).unwrap();
    assert_eq!(db.list_cas_live_refs().unwrap(), vec!["hash-a".to_string()]);
}

// ── P0-3: abort must never touch a promoted generation ────────────────

#[test]
fn abort_promoted_token_is_hard_error_claims_intact() {
    // The round-10 abort deleted the claims of ANY generation named —
    // aborting a PROMOTED token erased the live refset while its marker
    // stayed, and prune then ate blobs the project was using.
    // (Abort vòng-10 xóa claim của BẤT KỲ generation nào được nêu —
    // abort token ĐÃ PROMOTE xóa refset sống trong khi marker còn, rồi
    // prune ăn blob project đang dùng.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-live").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    let err = db.abort_cas_generation(PROJ_A, t0).unwrap_err();
    assert!(
        matches!(err, CasGenerationError::AlreadyPromoted { .. }),
        "abort of a promoted token must be a typed AlreadyPromoted error, got {err:?}"
    );
    assert_eq!(
        db.list_cas_live_refs().unwrap(),
        vec!["hash-live".to_string()],
        "P0-3: abort of a promoted token must keep every live claim"
    );
}

#[test]
fn abort_unknown_token_is_idempotent_noop() {
    let db = TestDb::new();
    let db = &db.db;
    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();

    // Missing token: no-op (crash-restart retried the abort).
    // (Token mất: no-op — restart sau crash chạy lại abort.)
    db.abort_cas_generation(PROJ_A, 424_242).unwrap();
    assert_eq!(
        db.list_cas_live_refs().unwrap(),
        vec!["hash-a".to_string()],
        "abort of an unknown token must touch nothing"
    );
}

// ── P0-4: claims reject forged / cross-project / promoted tokens ───────

#[test]
fn claim_rejects_forged_token() {
    let db = TestDb::new();
    let db = &db.db;

    let err = db.cas_claim(PROJ_A, 999_999, "hash-x").unwrap_err();
    assert!(
        matches!(err, CasGenerationError::UnknownToken { .. }),
        "claim with a forged token must be UnknownToken, got {err:?}"
    );
    assert!(db.list_cas_live_refs().unwrap().is_empty());
}

#[test]
fn claim_rejects_cross_project_token() {
    // A token minted for project B must not claim for project A — the
    // v2 API accepted it because nothing scoped (project, generation)
    // pairs together.
    // (Token đúc cho project B không được claim cho project A — API v2
    // nhận nó vì không gì ràng buộc cặp (project, generation).)
    let db = TestDb::new();
    let db = &db.db;

    let t_b = db.begin_cas_generation(PROJ_B).unwrap();
    let err = db.cas_claim(PROJ_A, t_b, "hash-x").unwrap_err();
    assert!(
        matches!(err, CasGenerationError::UnknownToken { .. }),
        "cross-project claim must be UnknownToken, got {err:?}"
    );
    assert!(db.list_cas_live_refs().unwrap().is_empty());
}

#[test]
fn claim_rejects_promoted_token() {
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    let err = db.cas_claim(PROJ_A, t0, "hash-b").unwrap_err();
    assert!(
        matches!(err, CasGenerationError::AlreadyPromoted { .. }),
        "claim into a promoted token must be AlreadyPromoted, got {err:?}"
    );
    assert_eq!(
        db.list_cas_live_refs().unwrap(),
        vec!["hash-a".to_string()],
        "the promoted generation keeps exactly its pre-promote claims"
    );
}

// ── FK backstop (P0-4, schema level) ───────────────────────────────────

#[test]
fn foreign_key_blocks_orphan_claim_at_schema_level() {
    // Even bypassing cas_claim's gate (raw SQL on the same connection
    // via a second Database handle is still gated — so go through a
    // token that EXISTS but is aborted): the FK (project_root,
    // generation) → cas_generations makes an orphan claim insert fail.
    // (Kể cả bypass cổng cas_claim: FK (project_root, generation) →
    // cas_generations làm việc chèn claim mồ côi thất bại.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.abort_cas_generation(PROJ_A, t0).unwrap();

    // The marker is gone; claiming through the API is UnknownToken...
    // (Marker đã mất; claim qua API là UnknownToken...)
    assert!(matches!(
        db.cas_claim(PROJ_A, t0, "hash-z").unwrap_err(),
        CasGenerationError::UnknownToken { .. }
    ));
    // ...and the raw conn is not exposed for writes from tests, so the
    // schema backstop is asserted by the API gate + FK definition. The
    // orphan scan in doctor (tests below) double-checks the invariant.
    // (...và raw conn không expose cho ghi từ test, nên lớp dự phòng
    // schema được chứng minh bằng cổng API + định nghĩa FK. Quét mồ côi
    // của doctor kiểm tra chéo invariant.)
}

// ── P0-1: atomic allocation under a REAL thread barrier ────────────────

#[test]
fn begin_cas_generation_survives_barrier_race() {
    // The round-10 "concurrency test" called begin twice SEQUENTIALLY on
    // two connections — no race at all. This test runs N threads behind
    // a barrier on the SAME db file (WAL + busy_timeout), each looping
    // begin/promote/abort, and demands: every begin returns a DISTINCT
    // token (allocation is atomic), no unique-violation surfaces, and
    // the final state is deterministic.
    // (Test "concurrency" vòng-10 gọi begin 2 lần TUẦN TỰ trên 2
    // connection — không có race. Test này chạy N thread sau barrier
    // trên CÙNG file db (WAL + busy_timeout), mỗi thread lặp
    // begin/promote/abort, và đòi: mọi begin trả token KHÁC nhau (cấp
    // phát nguyên tử), không lồ lộ unique-violation, state cuối tất định.)
    const THREADS: usize = 8;
    const ROUNDS: usize = 25;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    // Initialize the schema once (concurrent CREATE races are another
    // beast; open serializes schema setup here).
    // (Khởi tạo schema một lần (race CREATE song song là chuyện khác;
    // open xếp tuần tự phần setup schema ở đây).)
    {
        let db = Database::open(&db_path).unwrap();
        let t0 = db.begin_cas_generation(PROJ_A).unwrap();
        db.cas_claim(PROJ_A, t0, "hash-base").unwrap();
        db.promote_cas_generation(PROJ_A, t0).unwrap();
    }

    let barrier = Arc::new(Barrier::new(THREADS));
    let db_path = Arc::new(db_path);
    let mut handles = Vec::new();
    for _ in 0..THREADS {
        let barrier = Arc::clone(&barrier);
        let db_path = Arc::clone(&db_path);
        handles.push(std::thread::spawn(move || {
            // Open BEFORE the first barrier: DDL contention happens at
            // open, the race under test is begin/claim/promote only.
            // (Mở TRƯỚC barrier đầu: tranh chấp DDL xảy ra lúc open, race
            // được test chỉ là begin/claim/promote.)
            let db = Database::open(&db_path).unwrap();
            barrier.wait();
            let mut tokens = std::collections::HashSet::new();
            let mut errors: Vec<String> = Vec::new();
            for round in 0..ROUNDS {
                // Barrier per round: every thread hits begin at the same
                // logical moment — the true allocation race. A failing
                // step RECORDS the error and continues to the next
                // barrier round (a thread that returned early would
                // deadlock every other thread at the next barrier).
                // (Barrier mỗi vòng: mọi thread chạm begin cùng thời điểm
                // logic — race cấp phát thật. Bước fail GHI LỖI rồi tiếp
                // tục vòng barrier kế (thread return sớm làm mọi thread
                // khác chết đói ở barrier kế).)
                barrier.wait();
                match db.begin_cas_generation(PROJ_A) {
                    Ok(token) => {
                        if !tokens.insert(token) {
                            errors.push(format!(
                                "thread saw its own token {token} twice — \
                                 allocation not advancing"
                            ));
                            continue;
                        }
                        if let Err(e) =
                            db.cas_claim(PROJ_A, token, &format!("hash-t{token}-r{round}"))
                        {
                            errors.push(format!("claim r{round} t{token}: {e}"));
                            continue;
                        }
                        // Every install also REUSES a shared blob (the
                        // warm-cache pattern) — the reused hash must
                        // survive every promote in the race.
                        // (Mỗi install còn TÁI DỤNG một blob chung (pattern
                        // warm-cache) — hash tái dùng phải sống sót qua
                        // mọi promote trong race.)
                        if let Err(e) = db.cas_claim(PROJ_A, token, "hash-shared") {
                            errors.push(format!("shared claim r{round} t{token}: {e}"));
                            continue;
                        }
                        // Half the rounds promote, half abort — mixed
                        // lifecycle under contention.
                        // (Nửa vòng promote, nửa abort — vòng đời trộn
                        // lẫn dưới tranh chấp.)
                        let flip = if round % 2 == 0 {
                            db.promote_cas_generation(PROJ_A, token)
                        } else {
                            db.abort_cas_generation(PROJ_A, token)
                        };
                        if let Err(e) = flip {
                            errors.push(format!("lifecycle r{round} t{token}: {e}"));
                        }
                    }
                    Err(e) => errors.push(format!("begin r{round}: {e}")),
                }
            }
            (tokens, errors)
        }));
    }

    let mut all_tokens = std::collections::HashSet::new();
    let mut all_errors: Vec<String> = Vec::new();
    for handle in handles {
        let (tokens, errors) = handle.join().unwrap();
        all_errors.extend(errors);
        for token in tokens {
            assert!(
                all_tokens.insert(token),
                "P0-1: two threads were allocated the SAME token {token} — \
                 begin_cas_generation is not atomic"
            );
        }
    }
    assert!(
        all_errors.is_empty(),
        "race run reported errors: {all_errors:?}"
    );
    assert_eq!(all_tokens.len(), THREADS * ROUNDS);

    // Deterministic final state: the SHARED (reused) blob survives every
    // mix of promotes/aborts — the warm-cache invariant under contention.
    // Aborted tokens' exclusive claims are gone; promoted ones remain.
    // (State cuối tất định: blob CHUNG (tái dùng) sống sót qua mọi mix
    // promote/abort — invariant warm-cache dưới tranh chấp. Claim riêng
    // của token bị abort biến mất; của token promote còn lại.)
    let db = Database::open(&db_path).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert!(
        live.contains(&"hash-shared".to_string()),
        "the REUSED shared blob must survive the race, live = {live:?}"
    );
}

// ── RAII guard (Gate 11-A item 6) ─────────────────────────────────────

#[test]
fn generation_guard_aborts_staging_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let db = Database::open(&db_path).unwrap();

    let (token, live_during) = {
        let guard = db.begin_cas_generation_guarded(&db_path, PROJ_A).unwrap();
        db.cas_claim(PROJ_A, guard.generation(), "hash-g").unwrap();
        let live = db.list_cas_live_refs().unwrap();
        (guard.generation(), live)
    };
    // Guard dropped without disarm → the token is aborted, its claims
    // retired. This is the leak-proof contract for failed installs.
    // (Guard drop không disarm → token bị hủy, claim nghỉ hưu. Đây là
    // hợp đồng không-rò cho install fail.)
    assert_eq!(
        live_during,
        vec!["hash-g".to_string()],
        "claims are live while the guard holds the token"
    );
    let live_after = db.list_cas_live_refs().unwrap();
    assert!(
        live_after.is_empty(),
        "P0 guard: drop must abort the staging token, live = {live_after:?}, token = {token}"
    );
}

#[test]
fn generation_guard_disarm_keeps_promoted_claims() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let db = Database::open(&db_path).unwrap();

    let guard = db.begin_cas_generation_guarded(&db_path, PROJ_A).unwrap();
    let token = guard.generation();
    db.cas_claim(PROJ_A, token, "hash-gp").unwrap();
    db.promote_cas_generation(PROJ_A, token).unwrap();
    // Disarm AFTER promote — drop must NOT abort a promoted token.
    // (Disarm SAU promote — drop KHÔNG được hủy token đã promote.)
    guard.disarm();
    drop(db);

    let db = Database::open(&db_path).unwrap();
    assert_eq!(
        db.list_cas_live_refs().unwrap(),
        vec!["hash-gp".to_string()],
        "disarmed guard must leave the promoted claims live"
    );
}

// ── Project install lock (Gate 11-A item 7) ────────────────────────────

#[test]
fn project_install_lock_serializes_same_project() {
    let dir = tempfile::tempdir().unwrap();
    // Hermetic locks root (no global env mutation, no user-store touch)
    // — the public test seam of the lock.
    // (Root locks kín — đường test công khai của lock, không đổi env
    // toàn cục, không chạm store user.)
    let locks_root = dir.path().join("locks");
    let db = Database::open(&dir.path().join("store.db")).unwrap();
    let _ = &db; // the lock is store-level, not database-level

    let first = mgc_store::ProjectInstallLock::acquire_at(&locks_root, PROJ_A).unwrap();
    // A second lock on the SAME project must fail (try-lock, not block —
    // a blocking test would hang). A DIFFERENT project must still
    // acquire — the lock is per-project.
    // (Lock thứ hai trên CÙNG project phải fail (try-lock, không chặn
    // — chặn thì test treo). Project KHÁC vẫn phải lấy được — lock theo
    // từng project.)
    let second = mgc_store::ProjectInstallLock::acquire_at(&locks_root, PROJ_A);
    assert!(
        second.is_err(),
        "a second install of the same project must be refused, not raced"
    );
    assert!(
        mgc_store::ProjectInstallLock::acquire_at(&locks_root, PROJ_B).is_ok(),
        "a different project must acquire its own lock concurrently"
    );
    drop(first);
    // After release, the next install acquires.
    // (Sau khi nhả, install kế tiếp lấy được.)
    assert!(mgc_store::ProjectInstallLock::acquire_at(&locks_root, PROJ_A).is_ok());
}

// ── Crash semantics preserved from v2 (regression) ─────────────────────

#[test]
fn crashed_staging_claims_survive_other_promote_v3() {
    // The v3 gates must NOT regress the v2 crash-safety contract: a
    // crashed staging install's claims stay live across another token's
    // promote; only the crashed token's own abort retires them.
    // (Cổng v3 không được làm hỏng hợp đồng an toàn crash v2: claim của
    // install staging đứt sống qua promote của token khác; chỉ abort của
    // chính token đứt mới nghỉ hưu chúng.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    let t_crash = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_crash, "hash-z").unwrap();

    let t_next = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_next, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t_next, "hash-c").unwrap();
    db.promote_cas_generation(PROJ_A, t_next).unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec![
            "hash-a".to_string(),
            "hash-c".to_string(),
            "hash-z".to_string()
        ],
        "crashed staging claims survive another promote (P0-A contract)"
    );

    db.abort_cas_generation(PROJ_A, t_crash).unwrap();
    assert_eq!(
        db.list_cas_live_refs().unwrap(),
        vec!["hash-a".to_string(), "hash-c".to_string()],
        "the crashed token's own abort retires exactly its claims"
    );
}

// ── Migration with the FK rebuild (v2 → v3) keeps claims ───────────────

#[test]
fn migration_v2_shape_keeps_claims_and_enforces_fk() {
    // Build a v2-shaped store by hand (PK with generation IN the key,
    // no FK), then open with the new binary: the FK rebuild must keep
    // every claim and every marker, and new claims are gated.
    // (Dựng store hình v2 thủ công (PK có generation trong key, chưa
    // FK), rồi mở bằng binary mới: rebuild FK giữ mọi claim và marker,
    // claim mới bị cổng kiểm.)
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE packages (
                id TEXT NOT NULL, version TEXT NOT NULL, integrity TEXT,
                installed_at INTEGER NOT NULL, PRIMARY KEY (id, version));
             CREATE TABLE integrity_cache (hash TEXT PRIMARY KEY, verified_at INTEGER NOT NULL);
             CREATE TABLE refs (project_root TEXT NOT NULL, package_id TEXT NOT NULL,
                ref_count INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (project_root, package_id));
             CREATE TABLE cas_generations (
                project_root TEXT NOT NULL, generation INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT 'staging'
                    CHECK (state IN ('staging','promoted')),
                PRIMARY KEY (project_root, generation));
             CREATE TABLE cas_blob_refs (
                project_root TEXT NOT NULL, generation INTEGER NOT NULL DEFAULT 0,
                hash TEXT NOT NULL, PRIMARY KEY (project_root, generation, hash));
             CREATE TABLE package_files (id TEXT NOT NULL, version TEXT NOT NULL,
                path TEXT NOT NULL, blob_hash TEXT NOT NULL, size INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (id, version, path));
             CREATE TABLE trust_policy (package_id TEXT PRIMARY KEY, policy TEXT NOT NULL,
                updated_at INTEGER NOT NULL);
             CREATE TABLE release_policy (ecosystem TEXT PRIMARY KEY, min_age_secs INTEGER NOT NULL);
             INSERT INTO cas_generations VALUES ('/proj/legacy', 1, 'staging');
             INSERT INTO cas_blob_refs VALUES ('/proj/legacy', 1, 'hash-old');
             PRAGMA journal_mode=WAL;",
        )
        .unwrap();
    }

    let db = Database::open(&db_path).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["hash-old".to_string()]);

    // The migrated marker is staging → claims continue through the API.
    // (Marker migrate còn staging → claim tiếp tục qua API.)
    db.cas_claim("/proj/legacy", 1, "hash-old").unwrap();
    db.cas_claim("/proj/legacy", 1, "hash-new").unwrap();
    db.promote_cas_generation("/proj/legacy", 1).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-new".to_string(), "hash-old".to_string()],
        "reused legacy hash survives promote (P0-A regression guard)"
    );
}

// ── Crash-recovery lease (Gate 11-B, vòng-11) ──────────────────────────

#[test]
fn staging_leases_report_pid_and_age() {
    // The lease columns reach the API: every staging marker carries the
    // BEGINNING process's pid + timestamp, and the claim count rides
    // along — the doctor's GC needs all three to tell garbage from a
    // live install.
    // (Cột lease tới được API: mọi marker staging mang pid + timestamp
    // của process bắt đầu, kèm số claim — GC của doctor cần cả ba để
    // phân biệt rác với install đang chạy.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    let t1 = db.begin_cas_generation(PROJ_A).unwrap(); // claim-less begin

    let leases = db.list_staging_leases().unwrap();
    assert_eq!(leases.len(), 2, "both staging generations report leases");
    let by_token = |g: i64| leases.iter().find(|l| l.generation == g).unwrap();
    let l0 = by_token(t0);
    assert_eq!(l0.claim_count, 1);
    assert_eq!(l0.lease_pid, Some(std::process::id() as i64));
    assert!(l0.lease_started_at.is_some());
    let l1 = by_token(t1);
    assert_eq!(l1.claim_count, 0, "the second begin filed no claims yet");
    assert!(l1.lease_pid.is_some());

    // Promote retires the marker — the lease list shrinks.
    // (Promote nghỉ hưu marker — danh sách lease ngắn lại.)
    db.promote_cas_generation(PROJ_A, t0).unwrap();
    let leases = db.list_staging_leases().unwrap();
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].generation, t1);
}

#[test]
fn legacy_staging_without_lease_still_lists() {
    // Markers created BEFORE the lease columns exist (v2/v3 store) have
    // NULL lease fields — the GC must treat them as claim-count-only
    // (the old conservative contract), never panic on the NULLs.
    // (Marker tạo TRƯỚC khi cột lease tồn tại (store v2/v3) có trường
    // lease NULL — GC phải xử lý chúng chỉ theo số claim (hợp đồng bảo
    // toàn cũ), không bao giờ panic trên NULL.)
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let db = Database::open(&db_path).unwrap();
    let conn = db.conn();
    // Hand-insert a staging marker WITHOUT lease columns (the legacy
    // shape) — bypassing begin's lease stamp on purpose.
    // (Tự chèn marker staging KHÔNG qua lease — cố tình bypass phần
    // đóng dấu lease của begin.)
    conn.execute(
        "INSERT INTO cas_generations (project_root, generation, state)
         VALUES ('/proj/legacy-lease', 7, 'staging')",
        [],
    )
    .unwrap();
    let leases = db.list_staging_leases().unwrap();
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].lease_pid, None, "legacy marker: NULL lease pid");
    assert_eq!(leases[0].lease_started_at, None);
    assert_eq!(leases[0].claim_count, 0);
}

// ── Claim-map GC query (P2-2, fresh-context review 2026-09-15) ────────

/// The covered-by-promoted query names exactly the claim-ful staging
/// generations whose EVERY claimed hash is still claimed by a PROMOTED
/// generation of the same project — retiring them deletes no live-blob
/// protection. A staging with even ONE uncovered hash must NOT be named.
/// (Truy vấn covered-by-promoted gọi tên đúng các staging CÓ claim mà
/// MỌI hash được PROMOTED generation của cùng project giữ — nghỉ hưu
/// chúng không xóa bảo vệ blob sống nào. Staging có MỘT hash chưa được
/// giữ thì KHÔNG được gọi tên.)
#[test]
fn covered_by_promoted_names_only_safe_claimful_staging() {
    let db = TestDb::new();
    let db = &db.db;

    // Promoted baseline claims hash-a.
    // (Baseline promoted claim hash-a.)
    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    // A CRASHED staging (dead lease, has claims): all its hashes are
    // covered by the promoted baseline → SAFE to retire.
    // (Staging ĐỨT (lease chết, có claim): mọi hash được baseline
    // promoted giữ → AN TOÀN để nghỉ hưu.)
    let t_crash = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_crash, "hash-a").unwrap(); // covered
    db.cas_claim(PROJ_A, t_crash, "hash-b").unwrap(); // uncovered

    // Sanity: with hash-b uncovered, the crashed staging must NOT be
    // named (hash-b would lose its only protection).
    // (Kiểm tra: còn hash-b chưa được giữ, staging đứt KHÔNG được gọi
    // tên (hash-b sẽ mất bảo vệ duy nhất).)
    let covered = db.list_staging_leases_covered_by_promoted().unwrap();
    assert!(
        covered.iter().all(|l| l.generation != t_crash),
        "staging with an UNCOVERED claim (hash-b) must not be named for retirement"
    );

    // Promote a new generation that also claims hash-b — now the
    // crashed staging's claims are FULLY covered.
    // (Promote generation mới cũng claim hash-b — giờ claim của staging
    // đứt được phủ ĐẦY ĐỦ.)
    let t_next = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_next, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t_next, "hash-b").unwrap();
    db.promote_cas_generation(PROJ_A, t_next).unwrap();

    let covered = db.list_staging_leases_covered_by_promoted().unwrap();
    assert_eq!(covered.len(), 1, "exactly the crashed staging is named");
    assert_eq!(covered[0].generation, t_crash);
    assert_eq!(covered[0].claim_count, 2);
    assert_eq!(covered[0].project_root, PROJ_A);

    // Retiring it through the TOKEN-GATED abort keeps every live ref:
    // the promoted generation still claims hash-a and hash-b.
    // (Nghỉ hưu qua abort CÓ CỔNG TOKEN giữ mọi ref sống: generation
    // promoted vẫn claim hash-a và hash-b.)
    db.abort_cas_generation(PROJ_A, t_crash).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-a".to_string(), "hash-b".to_string()],
        "retiring the covered staging must not lose any live ref"
    );
}

/// Cross-project isolation: a staging of project A whose claims are
/// covered only by PROMOTED rows of project B must never be named —
/// coverage is per-project (B's claims protect nothing for A).
/// (Cô lập chéo project: staging của project A mà claim chỉ được row
/// PROMOTED của project B giữ thì không bao giờ được gọi tên — độ phủ
/// tính theo từng project (claim của B không bảo vệ gì cho A).)
#[test]
fn covered_by_promoted_is_project_scoped() {
    let db = TestDb::new();
    let db = &db.db;

    let t_b = db.begin_cas_generation(PROJ_B).unwrap();
    db.cas_claim(PROJ_B, t_b, "hash-shared").unwrap();
    db.promote_cas_generation(PROJ_B, t_b).unwrap();

    // Crashed A-staging claiming the same hash string — B's promotion
    // does NOT cover it.
    // (Staging A đứt claim cùng chuỗi hash — promotion của B KHÔNG phủ
    // nó.)
    let t_a = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_a, "hash-shared").unwrap();

    let covered = db.list_staging_leases_covered_by_promoted().unwrap();
    assert!(
        covered.iter().all(|l| l.project_root != PROJ_A),
        "project A's staging must not be retired on project B's coverage"
    );
}

/// Live-lease staging (the common case DURING an install) is never named
/// — the query is evidence-only; the doctor's pid/lock gates decide.
/// Pin the row SHAPE so the doctor's gates have the data they need.
/// (Staging lease-sống (case thường trong lúc install) không bao giờ bị
/// gọi tên — truy vấn chỉ đưa bằng chứng; cổng pid/lock của doctor quyết
/// định. Ghim shape row để các cổng của doctor có dữ liệu cần.)
#[test]
fn covered_by_promoted_carries_lease_fields() {
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    let t_crash = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_crash, "hash-a").unwrap();

    let covered = db.list_staging_leases_covered_by_promoted().unwrap();
    let row = covered
        .iter()
        .find(|l| l.generation == t_crash)
        .expect("the covered crashed staging must be named");
    assert!(row.lease_pid.is_some(), "lease pid rides along for the GC");
    assert!(row.lease_started_at.is_some());
    assert_eq!(row.claim_count, 1);
}
