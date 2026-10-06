#![allow(clippy::unwrap_used)]
//! Integration tests for CAS blob refcount + the generation-TOKEN protocol
//! (P0-A, adversarial review vòng-9): claims file into the caller's own
//! generation token, promote names the token — never a global MAX.
//! test riêng tại test/ (RULE §5).
//! (Test tích hợp refcount blob CAS + giao thức generation-TOKEN: claim rơi
//! vào generation token của chính caller, promote gọi đúng token — không có
//! MAX toàn cục.)

use mgc_store::Database;

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

#[test]
fn cas_claim_is_idempotent_per_project() {
    let db = TestDb::new();
    let db = &db.db;
    let t1 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t1, "blob-1").unwrap();
    db.cas_claim(PROJ_A, t1, "blob-1").unwrap();
    let t_b = db.begin_cas_generation(PROJ_B).unwrap();
    db.cas_claim(PROJ_B, t_b, "blob-1").unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["blob-1".to_string()]);
}

#[test]
fn cas_release_removes_single_project_claim() {
    let db = TestDb::new();
    let db = &db.db;
    let t_a = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_a, "blob-1").unwrap();
    let t_b = db.begin_cas_generation(PROJ_B).unwrap();
    db.cas_claim(PROJ_B, t_b, "blob-1").unwrap();
    db.cas_release(PROJ_A, "blob-1").unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["blob-1".to_string()]);

    db.cas_release(PROJ_B, "blob-1").unwrap();
    assert!(db.list_cas_live_refs().unwrap().is_empty());
}

#[test]
fn clear_all_cas_refs_resets_project_claims() {
    let db = TestDb::new();
    let db = &db.db;
    let t_a = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_a, "blob-1").unwrap();
    db.cas_claim(PROJ_A, t_a, "blob-2").unwrap();
    let t_b = db.begin_cas_generation(PROJ_B).unwrap();
    db.cas_claim(PROJ_B, t_b, "blob-1").unwrap();
    db.clear_all_cas_refs(PROJ_A).unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["blob-1".to_string()]);
}

#[test]
fn cas_refs_isolated_from_package_refs_table() {
    let db = TestDb::new();
    let db = &db.db;
    let root: std::path::PathBuf = PROJ_A.into();
    let pkg = mgc_types::PackageId::parse("react@18.2.0").unwrap();

    db.set_ref(root.to_str().unwrap(), &pkg).unwrap();
    let t_a = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t_a, "blob-a").unwrap();
    db.clear_ref(root.to_str().unwrap(), &pkg).unwrap();
    assert_eq!(db.list_cas_live_refs().unwrap(), vec!["blob-a".to_string()]);

    db.cas_release(PROJ_A, "blob-a").unwrap();
    assert!(db.list_cas_live_refs().unwrap().is_empty());
}

// === P0-A (vòng-10): hợp đồng token generation — các failure mode của
// giao thức v1 phải bị test ghim chết ===

#[test]
fn sequential_reinstall_reused_hash_keeps_live_claim() {
    // THE P0-A failure (vòng-9): baseline {A,B} → reinstall claims {A,C}
    // → promote → live MUST be {A,C}. The v1 schema keyed rows by
    // (project, hash): the re-claim of A was silently IGNORED (PK hit),
    // A stayed at the old generation, and promote DELETED A's claim even
    // though the new install still needed it — prune then ate a live blob.
    // (Đúng failure P0-A: baseline {A,B} → reinstall claim {A,C} →
    // promote → live PHẢI là {A,C}. Schema v1 khóa row theo (project,
    // hash): claim lại A bị bỏ qua âm thầm, A nằm lại generation cũ,
    // promote XÓA claim của A dù install mới vẫn cần — prune xóa blob sống.)
    let db = TestDb::new();
    let db = &db.db;

    // Baseline install {A, B}, fully promoted.
    // (Install baseline {A, B}, promote trọn vẹn.)
    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t0, "hash-b").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();
    assert_eq!(db.list_cas_live_refs().unwrap().len(), 2);

    // Reinstall {A, C} — A is REUSED (same bytes → same digest).
    // (Reinstall {A, C} — A được TÁI DỤNG (cùng bytes → cùng digest).)
    let t1 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t1, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t1, "hash-c").unwrap();
    db.promote_cas_generation(PROJ_A, t1).unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-a".to_string(), "hash-c".to_string()],
        "P0-A: reused hash A must survive promote of the reinstall (live = A ∪ C, B retired)"
    );
}

#[test]
fn reinstall_same_graph_keeps_every_claim() {
    // Reinstall of the SAME graph {A,B} must not lose a single claim —
    // the v1 bug deleted BOTH when the re-claims were all PK-ignored.
    // (Reinstall cùng graph {A,B} không được mất claim nào — bug v1 xóa
    // CẢ HAI khi mọi claim lại đều bị PK-ignore.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t0, "hash-b").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    let t1 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t1, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t1, "hash-b").unwrap();
    db.promote_cas_generation(PROJ_A, t1).unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-a".to_string(), "hash-b".to_string()],
        "same-graph reinstall must keep every claim"
    );
}

#[test]
fn crash_before_promote_leaves_old_union_new_live() {
    // Crash safety: an install that begins + claims but NEVER promotes
    // must leave old ∪ new claims live (over-retention, never a lost
    // blob). The NEXT install's promote is the only thing that retires.
    // (An toàn crash: install begin + claim nhưng KHÔNG promote phải để
    // lại claim cũ ∪ mới (giữ thừa, không mất blob). Chỉ promote của
    // install SAU mới nghỉ hưu.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    // Crashed install: staging claims exist, no promote.
    // (Install đứt: có claim staging, không promote.)
    let t1 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t1, "hash-z").unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert!(live.contains(&"hash-a".to_string()));
    assert!(live.contains(&"hash-z".to_string()));
}

#[test]
fn promote_of_crashed_staging_is_later_superseded_safely() {
    // A NEW install after a crashed one promotes its own token. The dead
    // staging generation CANNOT be distinguished from a still-running
    // install, so its claims must SURVIVE the other token's promote
    // (over-retention is the only safe default — P0-A contract: no promote
    // may delete another generation's staging claims). The retire path for
    // a dead staging generation is its OWN abort (or doctor GC, future
    // work) — demonstrated at the end of this test.
    // (Install MỚI sau install đứt promote token của chính nó. Staging gen
    // chết KHÔNG phân biệt được với install đang chạy nên claim của nó
    // PHẢI sống sót qua promote của token khác (giữ thừa là mặc định an
    // toàn duy nhất — hợp đồng P0-A: promote không được xóa claim staging
    // của generation khác). Đường nghỉ hưu cho staging gen chết là abort
    // CỦA CHÍNH NÓ (hoặc doctor GC, việc sau) — minh họa ở cuối test.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    let t1 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t1, "hash-z").unwrap();
    // (t1 crashes — never promoted, never aborted)

    let t2 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t2, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t2, "hash-c").unwrap();
    db.promote_cas_generation(PROJ_A, t2).unwrap();

    // The crashed staging's claim survives: over-retention, never a lost
    // blob. (hash-z is garbage the doctor will collect later.)
    // (Claim của staging đứt sống sót: giữ thừa, không bao giờ mất blob.
    // hash-z là rác doctor sẽ dọn sau.)
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec![
            "hash-a".to_string(),
            "hash-c".to_string(),
            "hash-z".to_string()
        ],
        "a crashed staging generation's claims must survive another token's promote (P0-A)"
    );

    // The dead install's own abort retires exactly its claims.
    // (Abort của chính install chết nghỉ hưu đúng claim của nó.)
    db.abort_cas_generation(PROJ_A, t1).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-a".to_string(), "hash-c".to_string()],
        "after the crashed token's abort, exactly the live graph remains"
    );
}

#[test]
fn concurrent_install_staging_claims_survive_the_other_promote() {
    // THE P0-A MAX-binding failure (vòng-9): install A (t1) running,
    // install B (t2) of the SAME project starts; B promotes FIRST. B's
    // promote must NOT delete t1's staging claims (t1 is still running);
    // when t1 promotes LAST it keeps everything its graph claims. Final
    // live = t1 ∪ t2 graph claims.
    // (Đúng failure MAX-binding P0-A: install A (t1) đang chạy, install B
    // (t2) cùng project bắt đầu; B promote TRƯỚC. Promote của B KHÔNG
    // được xóa claim staging của t1 (t1 còn đang chạy); t1 promote SAU
    // giữ mọi thứ graph nó claim. Live cuối = claim graph t1 ∪ t2.)
    let db = TestDb::new();
    let db = &db.db;

    // Baseline {A}.
    // (Baseline {A}.)
    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    // Two concurrent installs open their OWN tokens.
    // (Hai install song song mở token RIÊNG.)
    let t1 = db.begin_cas_generation(PROJ_A).unwrap();
    let t2 = db.begin_cas_generation(PROJ_A).unwrap();
    assert_ne!(t1, t2, "each install must own a distinct token");

    db.cas_claim(PROJ_A, t1, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t1, "hash-b").unwrap();
    db.cas_claim(PROJ_A, t2, "hash-a").unwrap();
    db.cas_claim(PROJ_A, t2, "hash-c").unwrap();

    // B (t2) finishes FIRST and promotes.
    // (B (t2) xong TRƯỚC và promote.)
    db.promote_cas_generation(PROJ_A, t2).unwrap();

    // t1's staging claims must still be live — t1 is still running.
    // (Claim staging của t1 vẫn phải live — t1 còn đang chạy.)
    let live = db.list_cas_live_refs().unwrap();
    assert!(
        live.contains(&"hash-b".to_string()),
        "in-flight install's staging claim must survive another install's promote (P0-A)"
    );

    // A (t1) finishes LAST and promotes.
    // (A (t1) xong SAU và promote.)
    db.promote_cas_generation(PROJ_A, t1).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert!(live.contains(&"hash-a".to_string()));
    assert!(live.contains(&"hash-b".to_string()));
    assert!(live.contains(&"hash-c".to_string()));
}

#[test]
fn abort_drops_only_own_generation_claims() {
    // Abort (P0-A): an install that gives up removes ONLY its own claims;
    // a sibling staging install and the baseline stay untouched.
    // (Abort: install bỏ cuộc chỉ xóa claim CỦA MÌNH; install staging
    // hàng xóm và baseline không bị đụng.)
    let db = TestDb::new();
    let db = &db.db;

    let t0 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db.promote_cas_generation(PROJ_A, t0).unwrap();

    let t1 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t1, "hash-b").unwrap();
    let t2 = db.begin_cas_generation(PROJ_A).unwrap();
    db.cas_claim(PROJ_A, t2, "hash-c").unwrap();

    db.abort_cas_generation(PROJ_A, t1).unwrap();
    // Abort is idempotent (crash-restart retried the abort).
    // (Abort idempotent (restart sau crash chạy lại abort).)
    db.abort_cas_generation(PROJ_A, t1).unwrap();

    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-a".to_string(), "hash-c".to_string()],
        "abort drops only the aborted token's claims"
    );
}

#[test]
fn two_connections_concurrent_begin_claim_promote() {
    // Two REAL separate SQLite connections (two processes): WAL journal
    // mode lets both claim into their own tokens; reverse-order promotes
    // must still end with every live-graph hash claimed. This is the
    // multiprocess contract the v1 MAX(generation) binding broke.
    // (Hai connection SQLite THẬT riêng (hai process): chế độ WAL cho cả
    // hai claim vào token riêng; promote NGƯỢC thứ tự vẫn phải kết thúc
    // với mọi hash graph sống được claim. Đây là hợp đồng multiprocess
    // mà gán MAX(generation) v1 phá.)
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");

    let db1 = Database::open(&db_path).unwrap();
    let db2 = Database::open(&db_path).unwrap();

    // Baseline {A} from connection 1.
    // (Baseline {A} từ connection 1.)
    let t0 = db1.begin_cas_generation(PROJ_A).unwrap();
    db1.cas_claim(PROJ_A, t0, "hash-a").unwrap();
    db1.promote_cas_generation(PROJ_A, t0).unwrap();

    // Two concurrent installs on separate connections.
    // (Hai install song song trên 2 connection riêng.)
    let t1 = db1.begin_cas_generation(PROJ_A).unwrap();
    let t2 = db2.begin_cas_generation(PROJ_A).unwrap();
    db1.cas_claim(PROJ_A, t1, "hash-a").unwrap();
    db1.cas_claim(PROJ_A, t1, "hash-b").unwrap();
    db2.cas_claim(PROJ_A, t2, "hash-a").unwrap();
    db2.cas_claim(PROJ_A, t2, "hash-c").unwrap();

    // Reverse-order promote: t2 first (db2), then t1 (db1).
    // (Promote ngược thứ tự: t2 trước (db2), rồi t1 (db1).)
    db2.promote_cas_generation(PROJ_A, t2).unwrap();
    db1.promote_cas_generation(PROJ_A, t1).unwrap();

    let live = db2.list_cas_live_refs().unwrap();
    assert!(live.contains(&"hash-a".to_string()));
    assert!(live.contains(&"hash-b".to_string()));
    assert!(live.contains(&"hash-c".to_string()));
}

#[test]
fn legacy_v0_database_migrates_to_token_schema() {
    // Migration (P0-A): a store.db written by the OLD schema (PK
    // (project, hash), NO generation column) must open, keep every claim
    // as generation 0, and support the full token protocol afterwards.
    // (Migration: store.db do schema CŨ ghi (PK (project, hash), KHÔNG
    // cột generation) phải mở được, giữ mọi claim thành generation 0, và
    // chạy trọn giao thức token sau đó.)
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");

    // Hand-build the v0 schema exactly as old binaries wrote it.
    // (Tự dựng schema v0 đúng như binary cũ từng ghi.)
    {
        use rusqlite::Connection;
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE cas_blob_refs (
                project_root TEXT NOT NULL,
                hash TEXT NOT NULL,
                PRIMARY KEY (project_root, hash)
            );
            INSERT INTO cas_blob_refs (project_root, hash) VALUES ('/proj/legacy', 'hash-old');",
        )
        .unwrap();
    }

    // Open with the new binary: migration runs, claims survive.
    // (Mở bằng binary mới: migration chạy, claim sống sót.)
    let db = Database::open(&db_path).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["hash-old".to_string()]);

    // The token protocol works on top of the migrated store.
    // (Giao thức token chạy trên store đã migrate.)
    let t1 = db.begin_cas_generation("/proj/legacy").unwrap();
    db.cas_claim("/proj/legacy", t1, "hash-old").unwrap();
    db.cas_claim("/proj/legacy", t1, "hash-new").unwrap();
    db.promote_cas_generation("/proj/legacy", t1).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-new".to_string(), "hash-old".to_string()],
        "reused legacy hash survives promote (the P0-A regression guard)"
    );
}

#[test]
fn legacy_v1_database_migrates_keeping_generation_numbers() {
    // Migration from v1 (vòng-7/8 shape: generation column, PK without
    // generation, single-row cas_generations): rows keep their generation
    // number; the migrated store supports the token protocol.
    // (Migration từ v1 (hình vòng-7/8: cột generation, PK không chứa
    // generation, cas_generations một row/project): row giữ số
    // generation; store đã migrate chạy được giao thức token.)
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");

    {
        use rusqlite::Connection;
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE cas_blob_refs (
                project_root TEXT NOT NULL,
                hash TEXT NOT NULL,
                generation INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (project_root, hash)
            );
            CREATE TABLE cas_generations (
                project_root TEXT PRIMARY KEY,
                generation INTEGER NOT NULL
            );
            INSERT INTO cas_generations (project_root, generation) VALUES ('/proj/v1', 2);
            INSERT INTO cas_blob_refs (project_root, hash, generation)
                VALUES ('/proj/v1', 'hash-gen2', 2);
            INSERT INTO cas_blob_refs (project_root, hash, generation)
                VALUES ('/proj/v1', 'hash-gen0', 0);",
        )
        .unwrap();
    }

    let db = Database::open(&db_path).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec!["hash-gen0".to_string(), "hash-gen2".to_string()],
        "v1 rows migrate with their claims intact"
    );
}
