/// SQLite-backed database for installed packages and integrity metadata.
use anyhow::Result;
use mgc_types::{PackageId, Version};
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DatabaseEntry {
    pub id: String,
    pub version: String,
    pub integrity: Option<String>,
    pub installed_at: u64,
}

/// One staging generation's crash-recovery lease (Gate 11-B, vòng-11):
/// WHO holds it (pid), WHEN it began, and how many claims it filed. The
/// doctor decides garbage vs live-install from ALL THREE — never from
/// age alone (an install can legitimately run for hours; a dead pid is
/// garbage the instant its claims are zero... but a dead pid WITH
/// claims may still over-protect blobs until its own abort — the doctor
/// reports those, repair is conservative).
/// (Lease phục hồi crash của một staging generation: AI giữ nó (pid),
/// KHI bắt đầu, và đã ghi bao nhiêu claim. Doctor quyết định rác-hay-
/// đang-chạy từ CẢ BA — không bao giờ chỉ theo tuổi (install hợp pháp
/// có thể chạy hàng giờ; pid chết là rác ngay khi claim bằng 0... nhưng
/// pid chết CÓ claim vẫn có thể đang bảo vệ thừa blob tới khi abort của
/// chính nó — doctor báo những cái đó, repair thiên về bảo toàn).)
#[derive(Debug, Clone)]
pub struct StagingLease {
    pub project_root: String,
    pub generation: i64,
    pub lease_pid: Option<i64>,
    pub lease_started_at: Option<i64>,
    pub claim_count: i64,
}

pub struct Database {
    conn: Connection,
}

/// Typed generation-token errors (Gate 11-A, vòng-11 audit): every
/// mutation of the token protocol must VERIFY the token BEFORE any row
/// is written — an unknown/forged/stale token is a typed error with
/// ZERO mutation, never a silent partial retire. This is the
/// "untrusted caller" contract: a corrupt or stale token can never
/// retire another generation's claims.
/// (Lỗi token generation có type (Gate 11-A): mọi mutation của giao
/// thức token phải VERIFY token TRƯỚC khi ghi row — token không
/// biết/giả/cũ là lỗi có type với KHÔNG mutation nào, không bao giờ
/// nghỉ hưu một phần âm thầm. Đây là hợp đồng "caller không tin
/// cậy": token hỏng hoặc cũ không bao giờ được nghỉ hưu claim của
/// generation khác.)
#[derive(Debug, thiserror::Error)]
pub enum CasGenerationError {
    #[error("cas generation token {generation} is not registered for project '{project_root}'")]
    UnknownToken {
        project_root: String,
        generation: i64,
    },
    #[error("cas generation token {generation} for project '{project_root}' is already promoted")]
    AlreadyPromoted {
        project_root: String,
        generation: i64,
    },
    #[error("database I/O error in cas generation protocol: {0}")]
    Io(String),
}

/// Probe whether `cas_generations` already carries `column`. Idempotent
/// re-open support — SQLite has no IF NOT EXISTS for ALTER ADD COLUMN, so
/// the doctor probes table_info first (race-free under the schema batch).
/// (Thăm dò cột đã tồn tại chưa — hỗ trợ mở lại idempotent, SQLite không
/// có IF NOT EXISTS cho ALTER ADD COLUMN, nên thăm dò table_info trước
/// (không race dưới batch schema).)
fn has_lease_column(conn: &Connection, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare("PRAGMA table_info(cas_generations)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in rows {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Add a lease column, tolerating ONLY the "duplicate column" race from a
/// concurrent migration. Every other error (disk I/O, locked, corrupt)
/// propagates — a store that cannot gain its lease schema must not open
/// half-migrated (fail-closed, Gate 11-B P0-3).
/// (Thêm cột lease, chỉ dung thứ lỗi "duplicate column" từ migration song
/// song. Mọi lỗi khác (I/O đĩa, khóa, hỏng) propagate — store không lấy
/// được schema lease thì không được mở nửa chừng (fail-closed, P0-3).)
fn add_lease_column(conn: &Connection, column: &str, ddl: &str) -> Result<()> {
    match conn.execute(ddl, []) {
        Ok(_) => Ok(()),
        Err(e) if e.to_string().contains("duplicate column name") => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "store.db schema migration failed: unable to add column {column}: {e}"
        )),
    }
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute_batch(
            "PRAGMA busy_timeout=5000;
            CREATE TABLE IF NOT EXISTS packages (
                id TEXT NOT NULL,
                version TEXT NOT NULL,
                integrity TEXT,
                installed_at INTEGER NOT NULL,
                PRIMARY KEY (id, version)
            );
            CREATE TABLE IF NOT EXISTS integrity_cache (
                hash TEXT PRIMARY KEY,
                verified_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS refs (
                project_root TEXT NOT NULL,
                package_id TEXT NOT NULL,
                ref_count INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (project_root, package_id)
            );
            CREATE TABLE IF NOT EXISTS cas_blob_refs (
                project_root TEXT NOT NULL,
                generation INTEGER NOT NULL DEFAULT 0,
                hash TEXT NOT NULL,
                PRIMARY KEY (project_root, generation, hash),
                FOREIGN KEY (project_root, generation)
                    REFERENCES cas_generations (project_root, generation)
                    ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_cas_blob_refs_hash ON cas_blob_refs (hash);
            CREATE TABLE IF NOT EXISTS cas_generations (
                project_root TEXT NOT NULL,
                generation INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT 'staging'
                    CHECK (state IN ('staging', 'promoted')),
                PRIMARY KEY (project_root, generation)
            );
            CREATE TABLE IF NOT EXISTS package_files (
                id TEXT NOT NULL,
                version TEXT NOT NULL,
                path TEXT NOT NULL,
                blob_hash TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (id, version, path)
            );
            CREATE INDEX IF NOT EXISTS idx_package_files_blob ON package_files (blob_hash);
            CREATE TABLE IF NOT EXISTS trust_policy (
                package_id TEXT PRIMARY KEY,
                policy TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS release_policy (
                ecosystem TEXT PRIMARY KEY,
                min_age_secs INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS cas_generation_seq (
                project_root TEXT PRIMARY KEY,
                last_issued INTEGER NOT NULL DEFAULT 0
            );
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;",
        )?;

        // Generation-token schema (P0-A, adversarial review vòng-9
        // 2026-09-14): the OLD schema kept PRIMARY KEY (project_root, hash)
        // while tagging rows with a generation column — an `INSERT OR
        // IGNORE` claim for a hash already claimed at generation 0 was
        // SILENTLY IGNORED, so the blob stayed at the OLD generation, and
        // promote (deleting every row outside the new generation) removed
        // the claim of a blob the NEW install still needed — even on a
        // plain sequential reinstall, no concurrency, no crash, no
        // attacker. The v2 schema keys rows by (project_root, generation,
        // hash) and tracks per-generation state: a claim ALWAYS lands in
        // its own generation and promote only retires rows this token no
        // longer vouches for. Old tables (PK without generation, no state
        // column) are migrated atomically below — rows keep their existing
        // generation number, stamped 'promoted'.
        //
        // (Schema generation-token (P0-A): schema CŨ giữ PRIMARY KEY
        // (project_root, hash) trong khi gắn tag row bằng cột generation —
        // claim `INSERT OR IGNORE` cho hash đã claim ở generation 0 bị BỎ
        // QUA ÂM THẦM, blob nằm lại generation CŨ, và promote (xóa mọi row
        // ngoài generation mới) xóa claim của blob mà install MỚI vẫn cần
        // — kể cả reinstall tuần tự, không cần concurrency/crash/attacker.
        // Schema v2 khóa row theo (project_root, generation, hash) + state
        // từng generation: claim LUÔN rơi vào generation của chính nó,
        // promote chỉ nghỉ hưu row mà token này không còn bảo chứng. Bảng
        // cũ được migrate nguyên tử bên dưới — row giữ số generation hiện
        // có, đóng dấu 'promoted'.)
        // Token-schema migration detection (P0-A): a store is CURRENT when
        // cas_blob_refs keys rows by (project_root, generation, hash) —
        // i.e. the `generation` column participates in the PRIMARY KEY.
        // v0 stores lack the column entirely; v1 stores (vòng-7/8) carry
        // it OUTSIDE the key. Probing the cas_generations state column is
        // NOT enough: the fresh-schema batch above creates a v2-shaped
        // cas_generations in a v0 store before this check runs.
        // (Phát hiện cần migration: store là MỚI khi cas_blob_refs khóa row
        // theo (project_root, generation, hash) — cột `generation` nằm
        // TRONG PRIMARY KEY. Store v0 không có cột; store v1 có cột nhưng
        // NGOÀI key. Thăm dò state của cas_generations KHÔNG đủ: batch
        // schema mới phía trên đã tạo cas_generations hình v2 trong store
        // v0 trước khi check này chạy.)
        let legacy_token_migration_needed = {
            let mut stmt = conn.prepare("PRAGMA table_info(cas_blob_refs)")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
            })?;
            let mut generation_in_pk = false;
            for col in rows {
                let (name, pk) = col?;
                if name == "generation" && pk > 0 {
                    generation_in_pk = true;
                }
            }
            !generation_in_pk
        };
        if legacy_token_migration_needed {
            let legacy_has_generation_column = {
                let mut stmt = conn.prepare("PRAGMA table_info(cas_blob_refs)")?;
                let mut has = false;
                let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for name in rows {
                    if name? == "generation" {
                        has = true;
                    }
                }
                has
            };
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS cas_generations_v2 (
                    project_root TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    state TEXT NOT NULL DEFAULT 'staging'
                        CHECK (state IN ('staging', 'promoted')),
                    PRIMARY KEY (project_root, generation)
                );
                CREATE TABLE IF NOT EXISTS cas_blob_refs_v2 (
                    project_root TEXT NOT NULL,
                    generation INTEGER NOT NULL DEFAULT 0,
                    hash TEXT NOT NULL,
                    PRIMARY KEY (project_root, generation, hash),
                    FOREIGN KEY (project_root, generation)
                        REFERENCES cas_generations_v2 (project_root, generation)
                        ON DELETE CASCADE
                );",
            )?;
            if legacy_has_generation_column {
                // v1 (vòng-7/8 shape): rows keep their generation number;
                // the v1 counter row maps to a 'staging' marker when it is
                // a never-promoted install (>0 — v1 promote resets to 0) —
                // over-retention until abort/doctor, never a lost blob.
                // Gen-0 rows are completed baselines → 'promoted' marker.
                // (v1: row giữ số generation; row counter v1 thành marker
                // 'staging' khi là install chưa promote (>0 — promote v1
                // reset về 0) — giữ thừa tới abort/doctor, không mất blob.
                // Row gen-0 là baseline đã hoàn tất → marker 'promoted'.)
                tx.execute_batch(
                    "INSERT OR IGNORE INTO cas_generations_v2 (project_root, generation, state)
                        SELECT project_root,
                               CASE WHEN generation = 0 THEN 0 ELSE generation END,
                               CASE WHEN generation = 0 THEN 'promoted' ELSE 'staging' END
                        FROM cas_generations;
                    INSERT OR IGNORE INTO cas_generations_v2 (project_root, generation, state)
                        SELECT DISTINCT project_root, 0, 'promoted' FROM cas_blob_refs
                        WHERE generation = 0;
                    INSERT OR IGNORE INTO cas_generations_v2 (project_root, generation, state)
                        SELECT DISTINCT project_root, generation, 'staging'
                        FROM cas_blob_refs WHERE generation > 0;
                    INSERT OR IGNORE INTO cas_blob_refs_v2 (project_root, generation, hash)
                        SELECT project_root, generation, hash FROM cas_blob_refs;
                    DROP TABLE cas_blob_refs;
                    DROP TABLE cas_generations;
                    ALTER TABLE cas_blob_refs_v2 RENAME TO cas_blob_refs;
                    ALTER TABLE cas_generations_v2 RENAME TO cas_generations;",
                )?;
            } else {
                // v0 (pre-generation): every claim becomes the generation-0
                // PROMOTED baseline of its project.
                // (v0: mọi claim thành baseline gen-0 PROMOTED của project.)
                tx.execute_batch(
                    "INSERT OR IGNORE INTO cas_generations_v2 (project_root, generation, state)
                        SELECT DISTINCT project_root, 0, 'promoted' FROM cas_blob_refs;
                    INSERT OR IGNORE INTO cas_blob_refs_v2 (project_root, generation, hash)
                        SELECT project_root, 0, hash FROM cas_blob_refs;
                    DROP TABLE cas_blob_refs;
                    DROP TABLE IF EXISTS cas_generations;
                    ALTER TABLE cas_blob_refs_v2 RENAME TO cas_blob_refs;
                    ALTER TABLE cas_generations_v2 RENAME TO cas_generations;",
                )?;
            }
            tx.commit()?;
        }

        // Sequence bootstrap (Gate 11-A, P0-1): a legacy store's markers
        // may already exceed 0 — the counter starts ABOVE the highest
        // surviving marker so tokens are never reused across the
        // migration. Fresh stores have no markers and skip this cheaply.
        // (Khởi động sequence: marker của store cũ có thể đã lớn hơn 0 —
        // bộ đếm khởi động TRÊN marker sống sót cao nhất để token không
        // bao giờ tái sử dụng qua migration. Store mới không có marker,
        // bỏ qua rẻ.)
        conn.execute(
            "INSERT INTO cas_generation_seq (project_root, last_issued)
             SELECT project_root, MAX(generation) FROM cas_generations
             GROUP BY project_root
             ON CONFLICT(project_root) DO UPDATE
             SET last_issued = MAX(last_issued, excluded.last_issued)",
            [],
        )?;

        // Crash-recovery lease columns (Gate 11-B, vòng-11 verdict): the
        // doctor's stale-staging GC must distinguish "an install is
        // RUNNING (pid alive, lease fresh)" from "the install DIED (pid
        // gone or lease ancient)" — age alone cannot. SQLite has no
        // IF NOT EXISTS for ALTER ADD COLUMN; probe table_info first
        // (idempotent across re-opens, race-free under the schema batch).
        // (Cột lease phục hồi crash: GC stale-staging của doctor phải
        // phân biệt "install đang CHẠY (pid sống, lease mới)" với
        // "install đã CHẾT (pid mất hoặc lease quá cũ)" — chỉ tuổi
        // không đủ. SQLite không có IF NOT EXISTS cho ALTER ADD COLUMN;
        // thăm dò table_info trước (idempotent qua các lần mở lại, không
        // race dưới batch schema).)
        {
            // P0-3 (Gate 11-B adversarial review 2026-09-15): the OLD code
            // swallowed EVERY ALTER TABLE error (`let _ = ...`) — a locked,
            // corrupt, or disk-full store silently opened with a MISSING
            // lease schema. Now: only the "duplicate column name" race is
            // tolerated; any other error propagates, and after migrating we
            // REVALIDATE (fail-closed) that both columns are present.
            // (P0-3: code CŨ nuốt MỌI lỗi ALTER TABLE (`let _ = ...`) —
            // store bị khóa/hỏng/đầy đĩa mở im lặng với schema lease THIẾU.
            // Giờ: chỉ race "duplicate column name" được dung thứ; lỗi khác
            // propagate, và sau migrate REVALIDATE (fail-closed) rằng cả
            // hai cột đã hiện diện.)
            if !has_lease_column(&conn, "lease_pid")? {
                add_lease_column(
                    &conn,
                    "lease_pid",
                    "ALTER TABLE cas_generations ADD COLUMN lease_pid INTEGER",
                )?;
            }
            if !has_lease_column(&conn, "lease_started_at")? {
                add_lease_column(
                    &conn,
                    "lease_started_at",
                    "ALTER TABLE cas_generations ADD COLUMN lease_started_at INTEGER",
                )?;
            }
            // REVALIDATE (fail-closed): a swallowed/concurrent ALTER must
            // never hand back a DB missing its lease schema — re-probe and
            // error if either column is still absent.
            // (REVALIDATE (fail-closed): ALTER bị nuốt/song song không bao
            // giờ được trả DB thiếu schema lease — thăm dò lại và lỗi nếu
            // cột nào vẫn thiếu.)
            let lease_pid_present = has_lease_column(&conn, "lease_pid")?;
            let lease_started_present = has_lease_column(&conn, "lease_started_at")?;
            if !lease_pid_present || !lease_started_present {
                return Err(anyhow::anyhow!(
                    "store.db schema migration failed: lease columns missing after ALTER \
                     (lease_pid={lease_pid_present}, lease_started_at={lease_started_present})"
                ));
            }
        }

        // Enable FK enforcement (Gate 11-A, P0-4): MUST run AFTER the
        // legacy migration above — PRAGMA foreign_keys is a no-op inside
        // a transaction, and the migration itself DROPs the parent tables
        // (legal only with FKs off). With this on, cas_blob_refs cannot
        // gain a row whose (project_root, generation) has no marker —
        // the schema-level guarantee behind cas_claim's token gate.
        // (Bật áp dụng FK (P0-4): phải chạy SAU migration legacy phía
        // trên — PRAGMA foreign_keys là no-op trong transaction, và chính
        // migration DROP bảng cha (hợp pháp chỉ khi FK tắt). Bật xong,
        // cas_blob_refs không thể có row mà (project_root, generation)
        // thiếu marker — bảo đảm tầng schema sau cổng token của cas_claim.)
        conn.pragma_update(None, "foreign_keys", true)?;

        Ok(Self { conn })
    }

    /// Raw connection — for StoreIndex full scans.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn insert_package(&self, id: &PackageId, integrity: Option<&str>) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.conn.execute(
            "INSERT OR REPLACE INTO packages (id, version, integrity, installed_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.name_str(), id.version().to_string(), integrity, now],
        )?;

        Ok(())
    }

    pub fn is_installed(&self, id: &PackageId) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM packages WHERE id = ?1 AND version = ?2")?;
        let count: i64 = stmt
            .query_row(params![id.name_str(), id.version().to_string()], |row| {
                row.get(0)
            })?;
        Ok(count > 0)
    }

    pub fn list_installed(&self) -> Result<Vec<DatabaseEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, version, integrity, installed_at FROM packages ORDER BY id")?;
        let entries = stmt.query_map([], |row| {
            Ok(DatabaseEntry {
                id: row.get(0)?,
                version: row.get(1)?,
                integrity: row.get(2)?,
                installed_at: row.get(3)?,
            })
        })?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        Ok(result)
    }

    pub fn remove_package(&self, id: &PackageId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM packages WHERE id = ?1 AND version = ?2",
            params![id.name_str(), id.version().to_string()],
        )?;
        Ok(())
    }

    pub fn verify_integrity(&self, hash: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM integrity_cache WHERE hash = ?1")?;
        let count: i64 = stmt.query_row(params![hash], |row| row.get(0))?;
        Ok(count > 0)
    }

    pub fn cache_integrity(&self, hash: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.conn.execute(
            "INSERT OR REPLACE INTO integrity_cache (hash, verified_at) VALUES (?1, ?2)",
            params![hash, now],
        )?;
        Ok(())
    }

    /// Mark (project_root, package_id) as referenced — install. (02 §2.2)
    pub fn set_ref(&self, project_root: &str, id: &PackageId) -> Result<()> {
        self.conn.execute(
            "INSERT INTO refs (project_root, package_id, ref_count) VALUES (?1, ?2, 1)
             ON CONFLICT(project_root, package_id)
             DO UPDATE SET ref_count = 1",
            params![project_root, id.to_string()],
        )?;
        Ok(())
    }

    /// Remove the project reference for a package — remove/uninstall.
    pub fn clear_ref(&self, project_root: &str, id: &PackageId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM refs WHERE project_root = ?1 AND package_id = ?2",
            params![project_root, id.to_string()],
        )?;
        Ok(())
    }

    /// Remove all references of a project — called before re-install so the
    /// refs table mirrors the current graph exactly (no stale entries).
    pub fn clear_all_refs(&self, project_root: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM refs WHERE project_root = ?1",
            params![project_root],
        )?;
        Ok(())
    }

    /// Return installed packages with no project referencing them —
    /// candidates for `mgc store prune` (02 §2.2).
    pub fn list_unreferenced(&self) -> Result<Vec<PackageId>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.version FROM packages p
             LEFT JOIN refs r ON r.package_id = p.id || '@' || p.version
             GROUP BY p.id, p.version
             HAVING COUNT(r.project_root) = 0",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let version: String = row.get(1)?;
            Ok(format!("{id}@{version}"))
        })?;
        let mut result = Vec::new();
        for row in rows {
            if let Ok(parsed) = PackageId::parse(&row?) {
                result.push(parsed);
            }
        }
        Ok(result)
    }

    /// Register that `project_root` references a CAS blob — idempotent per
    /// (project, generation, hash). The claim lands EXACTLY in the
    /// generation the caller's install token names (P0-A token protocol):
    /// a hash already claimed by a previous generation gets a SECOND row in
    /// this generation, so promote of THIS generation keeps protecting it —
    /// the v1 `INSERT OR IGNORE` against PK (project, hash) silently kept
    /// the old-generation row and promote deleted the claim of a blob the
    /// new install still needed (sequential reinstall, no concurrency).
    /// (Đăng ký project tham chiếu blob CAS — idempotent theo (project,
    /// generation, hash). Claim rơi ĐÚNG vào generation mà token install
    /// của caller chỉ định: hash đã được generation trước claim sẽ có row
    /// THỨ HAI ở generation này, nên promote của generation NÀY tiếp tục
    /// bảo vệ nó — `INSERT OR IGNORE` v1 với PK (project, hash) giữ nguyên
    /// row generation cũ và promote xóa claim của blob mà install mới vẫn
    /// cần (reinstall tuần tự, không cần concurrency).)
    pub fn cas_claim(
        &self,
        project_root: &str,
        generation: i64,
        hash: &str,
    ) -> std::result::Result<(), CasGenerationError> {
        // Capability gate FIRST (P0-4, vòng-11 audit): the token MUST exist,
        // belong to THIS project, and still be 'staging' — forged/cross-
        // project/promoted/missing tokens get a typed error and ZERO rows
        // written. The composite FK below is the schema-level backstop.
        // (Cổng năng lực TRƯỚC (P0-4): token PHẢI tồn tại, thuộc project
        // NÀY, còn 'staging' — token giả/sai project/đã promote/mất nhận
        // lỗi có type, KHÔNG ghi row nào. FK composite bên dưới là lớp dự
        // phòng ở tầng schema.)
        // `optional()` (rusqlite::OptionalExtension): NoRows becomes
        // None — the honest "token not found"; real I/O errors stay Err.
        // (`optional()`: NoRows thành None — "không có token" trung
        // thực; lỗi I/O thật giữ Err.)
        let state: Option<String> = self
            .conn
            .query_row(
                "SELECT state FROM cas_generations
                 WHERE project_root = ?1 AND generation = ?2",
                params![project_root, generation],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        match state.as_deref() {
            Some("staging") => {}
            Some(_) => {
                return Err(CasGenerationError::AlreadyPromoted {
                    project_root: project_root.to_string(),
                    generation,
                });
            }
            None => {
                return Err(CasGenerationError::UnknownToken {
                    project_root: project_root.to_string(),
                    generation,
                });
            }
        }
        self.conn
            .execute(
                "INSERT OR IGNORE INTO cas_blob_refs (project_root, generation, hash)
                 VALUES (?1, ?2, ?3)",
                params![project_root, generation, hash],
            )
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        Ok(())
    }

    /// Begin a NEW install generation for `project_root` and return its
    /// TOKEN (P0-A): the token is the ONLY identity of this install — claims
    /// must carry it, promote must receive it, and no MAX(generation) global
    /// peek exists anymore (the v1 MAX binding let a concurrent install
    /// hijack another install's claims). The new generation is 'staging'
    /// until promote; the previous generations stay visible to prune.
    /// (Bắt đầu generation install MỚI và trả TOKEN của nó: token là danh
    /// tính DUY NHẤT của install — claim phải mang nó, promote phải nhận
    /// đúng nó, không còn globally peek MAX(generation) (gán MAX v1 cho
    /// install song song cướp claim của install khác). Generation mới là
    /// 'staging' tới khi promote; generation trước vẫn hiện diện với prune.)
    pub fn begin_cas_generation(&self, project_root: &str) -> Result<i64> {
        // Atomic allocation (Gate 11-A, P0-1 of vòng-11 audit — FIXED after
        // the barrier race test PROVED BOTH naive versions raced):
        // (a) `SELECT MAX + 1` as two statements raced outright;
        // (b) `INSERT ... SELECT COALESCE(MAX+1)` still computed MAX from
        //     a pre-lock WAL snapshot;
        // (c) even a serialized MAX read REUSES numbers after an abort
        //     deletes the highest marker — the barrier test caught token
        // 26 handed to two threads. The fix is the AUTOINCREMENT
        // pattern: a `cas_generation_seq` counter that only GROWS
        // (abort never touches it), updated in the same BEGIN IMMEDIATE
        // transaction that inserts the marker. Write lock first, MAX
        // never consulted, tokens never reused — atomic by construction.
        // SQLITE_BUSY is retried by busy_timeout (set FIRST at open).
        // (Cấp phát nguyên tử (P0-1 — sửa sau khi test barrier race CHỨNG
        // MINH cả 3 bản ngây thơ đều đua): (a) `SELECT MAX + 1` tách 2
        // statement đua thẳng; (b) `INSERT ... SELECT COALESCE(MAX+1)` vẫn
        // tính MAX từ snapshot WAL trước lock; (c) kể cả đọc MAX tuần tự
        // vẫn TÁI SỬ DỤNG số sau khi abort xóa marker cao nhất — test bắt
        // được token 26 cấp cho 2 thread. Bản sửa là pattern
        // AUTOINCREMENT: bộ đếm `cas_generation_seq` chỉ TĂNG (abort
        // không bao giờ đụng), cập nhật trong cùng transaction BEGIN
        // IMMEDIATE chèn marker. Write lock trước, MAX không bao giờ bị
        // hỏi, token không bao giờ tái sử dụng — nguyên tử theo cấu
        // trúc. SQLITE_BUSY được retry bởi busy_timeout.)
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            // Upsert the sequence, RETURNING the issued number — one
            // statement under the write lock.
            // (Upsert sequence, TRẢ VỀ số đã cấp — một statement dưới
            // write lock.)
            let next: i64 = self.conn.query_row(
                "INSERT INTO cas_generation_seq (project_root, last_issued)
                     VALUES (?1, 1)
                 ON CONFLICT(project_root) DO UPDATE
                     SET last_issued = last_issued + 1
                 RETURNING last_issued",
                params![project_root],
                |row| row.get(0),
            )?;
            // Lease stamp (Gate 11-B): the staging marker records WHO
            // began it and WHEN — the doctor's stale-staging GC uses
            // pid-liveness + age to tell a live install from a dead one.
            // (Đóng dấu lease: marker staging ghi AI bắt đầu và KHI NÀO
            // — GC stale-staging của doctor dùng pid-còn-sống + tuổi để
            // phân biệt install đang chạy với install chết.)
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            self.conn.execute(
                "INSERT INTO cas_generations
                     (project_root, generation, state, lease_pid, lease_started_at)
                 VALUES (?1, ?2, 'staging', ?3, ?4)",
                params![project_root, next, std::process::id() as i64, now_secs],
            )?;
            Ok(next)
        })();
        match result {
            Ok(next) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(next)
            }
            Err(e) => {
                // Roll back best-effort; the error already explains the
                // failure. English-only console (RULE §7).
                // (Rollback cố gắng; lỗi đã giải thích thất bại. Console
                // tiếng Anh (RULE §7).)
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
    /// Atomically promote the TOKEN's staging generation (P0-A): in ONE
    /// transaction —
    /// 1. every claim of this token survives;
    /// 2. claims of OTHER generations die ONLY when the token's generation
    ///    still vouches for that hash (dedup) or the other generation is
    ///    NOT a live staging install (an in-flight install's staging claims
    ///    are protected — over-retention, never deletion of a live blob);
    /// 3. the token's generation flips to 'promoted', all OTHER promoted
    ///    generations of this project are retired (their rows are gone per
    ///    rule 2), older still-staging generations are LEFT ALONE for their
    ///    own promote/abort to resolve.
    ///
    /// A crash before this point leaves old ∪ new claims live — always SAFE
    /// to prune against.
    ///
    /// Thăng cấp nguyên tử generation của TOKEN: trong MỘT transaction —
    /// (1) mọi claim của token sống sót; (2) claim của generation KHÁC chỉ
    /// chết khi generation của token vẫn bảo chứng hash đó (dedup) hoặc
    /// generation kia KHÔNG phải install staging đang chạy (claim staging
    /// của install đang chạy được bảo vệ — giữ thừa, không xóa blob sống);
    /// (3) generation của token thành 'promoted', mọi generation 'promoted'
    /// khác của project nghỉ hưu, staging cũ kia giữ nguyên cho
    /// promote/abort của riêng nó giải quyết. Crash trước điểm này để lại
    /// claim cũ ∪ mới — luôn AN TOÀN cho prune.
    pub fn promote_cas_generation(
        &self,
        project_root: &str,
        generation: i64,
    ) -> std::result::Result<(), CasGenerationError> {
        // IMMEDIATE transaction (Gate 11-B hardening): promote reads the
        // token state BEFORE writing — a DEFERRED transaction upgrades
        // read→write mid-flight and hits BUSY_SNAPSHOT (unretryable under
        // WAL contention, observed live by the barrier race test). Taking
        // the write lock up front makes the whole verify+retire+flip one
        // serialized unit; SQLITE_BUSY retries via busy_timeout.
        // (Transaction IMMEDIATE: promote đọc state token TRƯỚC khi ghi —
        // transaction DEFERRED nâng read→write giữa chừng và dính
        // BUSY_SNAPSHOT (không retry được dưới tranh chấp WAL, test barrier
        // bắt được lúc chạy thật). Lấy write lock ngay từ đầu biến toàn bộ
        // verify+retire+flip thành một đơn vị xếp tuần tự; SQLITE_BUSY
        // retry qua busy_timeout.)
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        let result = self.promote_cas_generation_impl(project_root, generation);
        match result {
            Ok(()) => self
                .conn
                .execute_batch("COMMIT")
                .map_err(|e| CasGenerationError::Io(e.to_string())),
            Err(e) => {
                // Best-effort rollback: the primary error already explains
                // the failure; a failed rollback adds nothing and must not
                // mask the original error. (Rollback cố gắng: lỗi gốc đã
                // giải thích thất bại; rollback lỗi không thêm gì và không
                // được che lỗi gốc.)
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Verify+retire+flip body of promote — runs INSIDE the caller's
    /// IMMEDIATE transaction (write lock already held).
    /// (Thân verify+retire+flip của promote — chạy BÊN TRONG transaction
    /// IMMEDIATE của caller (write lock đã giữ).)
    fn promote_cas_generation_impl(
        &self,
        project_root: &str,
        generation: i64,
    ) -> std::result::Result<(), CasGenerationError> {
        // VERIFY the token FIRST — zero mutation on a bad token (Gate 11-A,
        // P0-2 of vòng-11 audit): the old code ran its DELETEs before
        // checking the token exists, so promote(project, 999999) retired
        // every claim of every promoted generation older than the forged
        // number and still returned Ok(()). Now: unknown token → typed
        // error, zero rows touched; already-promoted token → verified
        // idempotent no-op (re-promote of a promoted token is safe by
        // design, but must be PROVEN, not assumed); staging token → the
        // only state allowed to mutate.
        // (VERIFY token TRƯỚC TIÊN — không mutation nào với token sai
        // (P0-2): code cũ chạy DELETE trước khi check token tồn tại, nên
        // promote(project, 999999) nghỉ hưu mọi claim của mọi generation
        // promoted cũ hơn con số giả đó rồi vẫn Ok(()). Giờ: token không
        // biết → lỗi có type, không đụng row nào; token đã promoted →
        // no-op idempotent ĐƯỢC KIỂM CHỨNG (re-promote an toàn theo thiết
        // kế nhưng phải CHỨNG MINH, không mặc định); token staging → trạng
        // thái duy nhất được phép mutate.)
        let state: Option<String> = self
            .conn
            .query_row(
                "SELECT state FROM cas_generations
                 WHERE project_root = ?1 AND generation = ?2",
                params![project_root, generation],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        match state.as_deref() {
            Some("staging") => {}
            Some(_) => {
                // Already-promoted: verified no-op. The retire rules below
                // would be re-runs (no-ops once clean), but PROVING the
                // state first means a stale token can never reach them.
                // (Đã promoted: no-op đã kiểm chứng. Các luật nghỉ hưu bên
                // dưới chỉ chạy lại (no-op khi sạch), nhưng KIỂM CHỨNG state
                // trước nghĩa là token cũ không bao giờ chạm tới chúng.)
                return Ok(());
            }
            None => {
                return Err(CasGenerationError::UnknownToken {
                    project_root: project_root.to_string(),
                    generation,
                });
            }
        }
        // Retire rows the token no longer vouches for — bounded to
        // generations OLDER than this token (P0-A concurrency): a NEWER
        // generation belongs to an install that opened AFTER us (possibly
        // still running, possibly already promoted); retiring ITS rows on
        // our promote is exactly the reverse-order-promote race that ate
        // live blobs. Newer generations keep their rows; their own
        // promote/abort resolves them. Among older generations, only rows
        // whose hash has NO row in our token die, and NEVER the rows of a
        // still-STAGING generation (a crashed older install keeps
        // over-protecting until doctor/abort collects it — safe).
        // (Nghỉ hưu row mà token không còn bảo chứng — chỉ giới hạn trong
        // các generation CŨ HƠN token này: generation MỚI HƠN thuộc install
        // mở SAU mình (có thể đang chạy hoặc đã promote); nghỉ hưu row của
        // nó trên promote của mình chính là race promote-ngược-thứ-tự từng
        // xóa blob sống. Generation mới hơn giữ row; promote/abort của
        // chính chúng giải quyết. Trong các generation cũ hơn, chỉ row mà
        // hash không còn row ở token mình bị xóa, và KHÔNG BAO GIỜ xóa row
        // của generation còn STAGING (install cũ đứt giữ thừa tới khi
        // doctor/abort dọn — an toàn).)
        self.conn
            .execute(
                "DELETE FROM cas_blob_refs WHERE project_root = ?1
             AND generation < ?2
             AND NOT EXISTS (
                 SELECT 1 FROM cas_blob_refs t
                 WHERE t.project_root = ?1 AND t.generation = ?2
                   AND t.hash = cas_blob_refs.hash
             )
             AND NOT EXISTS (
                 SELECT 1 FROM cas_generations s
                 WHERE s.project_root = ?1 AND s.generation = cas_blob_refs.generation
                   AND s.state = 'staging'
             )",
                params![project_root, generation],
            )
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        // Collapse the remaining OLDER promoted generations into this
        // token (their surviving rows are exactly the dedup-protected
        // ones) and retire their markers — one hash, one live row.
        // (Gộp các generation promoted CŨ HƠN còn sót về token này (row
        // sống sót của chúng chính là các row dedup) và nghỉ hưu marker —
        // mỗi hash một row sống.)
        self.conn
            .execute(
                "DELETE FROM cas_blob_refs WHERE project_root = ?1 AND generation < ?2
             AND generation IN (
                 SELECT generation FROM cas_generations
                 WHERE project_root = ?1 AND state = 'promoted'
             )",
                params![project_root, generation],
            )
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        self.conn
            .execute(
                "DELETE FROM cas_generations
             WHERE project_root = ?1 AND generation < ?2 AND state = 'promoted'",
                params![project_root, generation],
            )
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        // Flip THIS token to promoted — and CHECK the affected row count
        // (Gate 11-A, P0-2): the verify above proved 'staging', so this
        // UPDATE must touch exactly 1 row; anything else is corruption
        // between statements and must abort the transaction rather than
        // commit a half-promote.
        // (Đổi token NÀY thành promoted — VÀ KIỂM TRA affected row count
        // (P0-2): verify phía trên đã chứng minh 'staging', nên UPDATE này
        // phải đụng đúng 1 row; khác đi là corruption giữa các statement
        // và phải hủy transaction thay vì commit một promote nửa vời.)
        let flipped = self
            .conn
            .execute(
                "UPDATE cas_generations SET state = 'promoted'
                 WHERE project_root = ?1 AND generation = ?2 AND state = 'staging'",
                params![project_root, generation],
            )
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        if flipped != 1 {
            return Err(CasGenerationError::Io(format!(
                "promote flip touched {flipped} row(s), expected exactly 1 — \
                 transaction rolled back"
            )));
        }
        Ok(())
    }

    /// Abort a staging generation (P0-A): drop ITS OWN claims only — the
    /// install that owns the token decided to give up; nobody else's
    /// generation is touched (a concurrent install's staging claims stay).
    ///
    /// State gate FIRST (Gate 11-A, P0-3 of vòng-11 audit): the old code
    /// deleted the token's claims BEFORE checking the marker's state, so
    /// abort(promoted_token) erased every claim of the LIVE promoted
    /// generation while its marker stayed — prune then ate blobs the
    /// project was actively using. Now the abort runs in ONE transaction
    /// that (1) verifies the token exists, (2) verifies it is 'staging',
    /// and only then (3) deletes claims + marker together. Promoted →
    /// typed hard error, zero mutation. Missing → idempotent no-op.
    /// (Hủy một staging generation: chỉ xóa claim CỦA CHÍNH nó — install
    /// sở hữu token quyết định bỏ; không đụng generation của ai khác (claim
    /// staging của install song song giữ nguyên). Cổng state TRƯỚC TIÊN
    /// (P0-3): code cũ xóa claim của token TRƯỚC khi check state của
    /// marker, nên abort(promoted_token) xóa sạch claim của generation
    /// promoted ĐANG SỐNG trong khi marker vẫn còn — prune rồi xóa blob
    /// mà project đang dùng. Giờ abort chạy trong MỘT transaction: (1)
    /// verify token tồn tại, (2) verify còn 'staging', rồi MỚI (3) xóa
    /// claim + marker cùng nhau. Promoted → lỗi cứng có type, không
    /// mutation. Mất → no-op idempotent.)
    pub fn abort_cas_generation(
        &self,
        project_root: &str,
        generation: i64,
    ) -> std::result::Result<(), CasGenerationError> {
        // IMMEDIATE transaction — same rationale as promote (see above):
        // verify-then-write in a deferred transaction upgrades locks
        // mid-flight and dies to BUSY_SNAPSHOT under WAL contention.
        // (Transaction IMMEDIATE — cùng lý do như promote (xem trên):
        // verify-rồi-ghi trong deferred transaction nâng lock giữa chừng
        // và chết vì BUSY_SNAPSHOT dưới tranh chấp WAL.)
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| CasGenerationError::Io(e.to_string()))?;
        let result = (|| {
            let state: Option<String> = self
                .conn
                .query_row(
                    "SELECT state FROM cas_generations
                     WHERE project_root = ?1 AND generation = ?2",
                    params![project_root, generation],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| CasGenerationError::Io(e.to_string()))?;
            match state.as_deref() {
                Some("staging") => {}
                Some(_) => {
                    return Err(CasGenerationError::AlreadyPromoted {
                        project_root: project_root.to_string(),
                        generation,
                    });
                }
                None => {
                    // Idempotent no-op (crash-restart retried the abort
                    // after the token was already retired).
                    // (No-op idempotent — restart sau crash chạy lại
                    // abort sau khi token đã nghỉ hưu.)
                    return Ok(());
                }
            }
            self.conn
                .execute(
                    "DELETE FROM cas_blob_refs WHERE project_root = ?1 AND generation = ?2",
                    params![project_root, generation],
                )
                .map_err(|e| CasGenerationError::Io(e.to_string()))?;
            let retired = self
                .conn
                .execute(
                    "DELETE FROM cas_generations
                     WHERE project_root = ?1 AND generation = ?2 AND state = 'staging'",
                    params![project_root, generation],
                )
                .map_err(|e| CasGenerationError::Io(e.to_string()))?;
            if retired != 1 {
                return Err(CasGenerationError::Io(format!(
                    "abort retire touched {retired} row(s), expected exactly 1 — \
                     transaction rolled back"
                )));
            }
            Ok(())
        })();
        match result {
            Ok(()) => self
                .conn
                .execute_batch("COMMIT")
                .map_err(|e| CasGenerationError::Io(e.to_string())),
            Err(e) => {
                // Best-effort rollback: the primary error already explains
                // the failure; a failed rollback adds nothing and must not
                // mask the original error. (Rollback cố gắng: lỗi gốc đã
                // giải thích thất bại; rollback lỗi không thêm gì và không
                // được che lỗi gốc.)
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Crash-recovery lease query (Gate 11-B): staging generations with
    /// their lease — (project, generation, pid, started_at, claim_count).
    /// A staging generation is GARBAGE only when BOTH the lease holder is
    /// DEAD (pid not alive — checked by the caller, this crate stays
    /// platform-clean) AND the lease is older than the grace window AND
    /// it holds zero claims; anything else may be a LIVE install.
    /// (Truy vấn lease phục hồi crash: các staging generation kèm lease.
    /// Một staging generation là RÁC chỉ khi CẢ holder lease ĐÃ CHẾT (pid
    /// không sống — caller kiểm tra, crate này giữ sạch platform) VÀ lease
    /// cũ hơn cửa sổ xử lý VÀ không giữ claim nào; còn lại có thể là
    /// install đang CHẠY.)
    pub fn list_staging_leases(&self) -> Result<Vec<StagingLease>> {
        let mut stmt = self.conn.prepare(
            "SELECT g.project_root, g.generation, g.lease_pid, g.lease_started_at,
                    (SELECT COUNT(*) FROM cas_blob_refs r
                     WHERE r.project_root = g.project_root
                       AND r.generation = g.generation) AS claims
             FROM cas_generations g
             WHERE g.state = 'staging'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(StagingLease {
                project_root: row.get(0)?,
                generation: row.get(1)?,
                lease_pid: row.get(2)?,
                lease_started_at: row.get(3)?,
                claim_count: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Claim-map GC query (P2-2, fresh-context review 2026-09-15): the
    /// doctor's old lease GC NEVER retired a claim-FUL staging
    /// generation — a crash between the first claim and promote leaked
    /// its claims FOREVER (safe over-retention, but unbounded). This
    /// query names the RETIRABLE subset with full evidence:
    /// a claim-ful staging generation is garbage iff
    ///   (a) its lease holder is DEAD or the lease is ancient (the
    ///       caller checks pid/age — the row carries both), AND
    ///   (b) EVERY hash it claims is still claimed by a PROMOTED
    ///       generation of the same project — i.e. retiring it deletes
    ///       not a single live-blob protection (the newest promoted
    ///       refset subsumes it).
    /// Rows returned: (project, generation, pid, started_at, claims).
    /// The caller still gates on pid-liveness + grace and performs the
    /// retire through `abort_cas_generation` (token gates stay intact).
    /// (Truy vấn GC claim-map (P2-2): GC lease cũ của doctor KHÔNG BAO
    /// GIỜ nghỉ hưu staging CÓ claim — crash giữa claim đầu và promote
    /// rò claim VĨNH VIỄN (giữ thừa an toàn nhưng không giới hạn). Truy
    /// vấn này gọi tên tập NGHỈ HƯU ĐƯỢC với bằng chứng đầy đủ: staging
    /// có claim là rác iff (a) holder lease CHẾT hoặc lease cổ (caller
    /// check pid/tuổi — row mang cả hai), VÀ (b) MỌI hash nó claim vẫn
    /// được generation PROMOTED của cùng project claim — nghỉ hưu nó
    /// không xóa một bảo vệ blob sống nào (refset promoted mới nhất thâu
    /// hẹp nó). Caller vẫn chặn theo pid/tuổi và nghỉ hưu qua
    /// abort_cas_generation (cổng token giữ nguyên).)
    pub fn list_staging_leases_covered_by_promoted(&self) -> Result<Vec<StagingLease>> {
        let mut stmt = self.conn.prepare(
            "SELECT g.project_root, g.generation, g.lease_pid, g.lease_started_at,
                    (SELECT COUNT(*) FROM cas_blob_refs r
                     WHERE r.project_root = g.project_root
                       AND r.generation = g.generation) AS claims
             FROM cas_generations g
             WHERE g.state = 'staging'
               AND EXISTS (
                   SELECT 1 FROM cas_blob_refs r
                   WHERE r.project_root = g.project_root
                     AND r.generation = g.generation
               )
               AND NOT EXISTS (
                   -- any claimed hash NOT covered by a promoted generation
                   -- of this project → NOT safe to retire
                   -- (hash được claim mà KHÔNG promoted generation nào của
                   -- project này giữ → KHÔNG an toàn để nghỉ hưu)
                   SELECT 1 FROM cas_blob_refs r
                   WHERE r.project_root = g.project_root
                     AND r.generation = g.generation
                     AND NOT EXISTS (
                         SELECT 1 FROM cas_blob_refs p
                         JOIN cas_generations m
                           ON m.project_root = p.project_root
                          AND m.generation = p.generation
                          AND m.state = 'promoted'
                         WHERE p.project_root = r.project_root
                           AND p.hash = r.hash
                     )
               )",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(StagingLease {
                project_root: row.get(0)?,
                generation: row.get(1)?,
                lease_pid: row.get(2)?,
                lease_started_at: row.get(3)?,
                claim_count: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// TEST-ONLY destructive clear (Gate 11-A tightening, vòng-11
    /// verdict): NO production caller exists (verified by call-graph scan
    /// — only refcount.rs tests use it). Under the generation-token
    /// protocol this deletes the claims of EVERY generation of a project
    /// — staging installs included — exactly the live-blob loss the token
    /// gates exist to prevent. It is now feature-gated so production
    /// builds cannot link it; the protocol's only retire paths are
    /// promote/abort (verified) and the doctor's stale-staging GC.
    /// (Clear phá hủy CHỈ-CHO-TEST: không còn caller production nào
    /// (quét call-graph xác minh — chỉ test refcount.rs dùng). Dưới giao
    /// thức token, hàm này xóa claim của MỌI generation của project — cả
    /// install staging — đúng chỗ mất blob sống mà các cổng token tồn tại
    /// để chặn. Giờ khóa theo feature để build production không link
    /// được; đường nghỉ hưu duy nhất của giao thức là promote/abort (đã
    /// verify) và GC stale-staging của doctor.)
    #[cfg(feature = "test-util")]
    pub fn clear_all_cas_refs(&self, project_root: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM cas_blob_refs WHERE project_root = ?1",
            params![project_root],
        )?;
        Ok(())
    }

    /// TEST-ONLY single-claim removal (Gate 11-A tightening): same
    /// rationale as `clear_all_cas_refs` — it deletes the claim row of
    /// EVERY generation holding that hash (staging installs included),
    /// bypassing the token gates. Production code must retire claims via
    /// promote (graph changes) or abort (install failure).
    /// (Xóa 1 claim CHỈ-CHO-TEST: cùng lý do như `clear_all_cas_refs` —
    /// xóa row claim của MỌI generation đang giữ hash đó (kể cả staging),
    /// vượt qua cổng token. Code production phải nghỉ hưu claim qua
    /// promote (graph đổi) hoặc abort (install fail).)
    #[cfg(feature = "test-util")]
    pub fn cas_release(&self, project_root: &str, hash: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM cas_blob_refs WHERE project_root = ?1 AND hash = ?2",
            params![project_root, hash],
        )?;
        Ok(())
    }

    /// Blob hashes with at least one live project claim — must NOT be pruned.
    /// Live = ANY generation (staging or promoted): an in-flight install's
    /// claims protect its blobs, and the previous generation protects the
    /// blobs the new install will reuse (P0-C).
    /// (Hash blob còn ít nhất 1 project claim — không được xóa. Live =
    ///  BẤT KỲ generation nào (staging hay đã promote): claim của install
    ///  đang chạy bảo vệ blob của nó, generation trước bảo vệ blob mà
    ///  install mới sẽ tái dùng (P0-C).)
    pub fn list_cas_live_refs(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT hash FROM cas_blob_refs ORDER BY hash")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Replace the file listing for one package (schema + source of truth for
    /// StoreIndex). Atomic within a transaction.
    pub fn replace_package_files(
        &self,
        id: &PackageId,
        files: &[(String, String, u64)],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM package_files WHERE id = ?1 AND version = ?2",
            params![id.name_str(), id.version().to_string()],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO package_files (id, version, path, blob_hash, size) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (path, hash, size) in files {
                stmt.execute(params![
                    id.name_str(),
                    id.version().to_string(),
                    path,
                    hash,
                    size
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Full file listing for a package — ordered by path. Empty = not indexed.
    pub fn list_package_files(&self, id: &PackageId) -> Result<Vec<(String, String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, blob_hash, size FROM package_files
             WHERE id = ?1 AND version = ?2 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![id.name_str(), id.version().to_string()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Every blob hash referenced by any indexed package.
    pub fn list_all_blob_hashes(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT blob_hash FROM package_files ORDER BY blob_hash")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Number of indexed packages — cheap health check for StoreIndex.rebuild.
    pub fn count_indexed_packages(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT id || '#' || version) FROM package_files",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    // ── T5 security trust gate ──────────────────────────────────────────

    /// Policy recorded for a package. `package_id` is the raw key: `name` or
    /// `name@version`.
    pub fn get_trust_policy(&self, package_id: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT policy FROM trust_policy WHERE package_id = ?1")?;
        let mut rows = stmt.query_map(params![package_id], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(policy)) => Ok(Some(policy)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Record approve/deny for a package (`name` covers all versions,
    /// `name@version` covers exactly one).
    pub fn upsert_trust_policy(&self, package_id: &str, policy: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.conn.execute(
            "INSERT OR REPLACE INTO trust_policy (package_id, policy, updated_at) VALUES (?1, ?2, ?3)",
            params![package_id, policy, now],
        )?;
        Ok(())
    }

    pub fn list_trust_policies(&self) -> Result<Vec<(String, String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT package_id, policy, updated_at FROM trust_policy ORDER BY package_id",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn clear_trust_policy(&self, package_id: &str) -> Result<bool> {
        let n = self.conn.execute(
            "DELETE FROM trust_policy WHERE package_id = ?1",
            params![package_id],
        )?;
        Ok(n > 0)
    }

    /// Remove policies for packages no longer installed (bare name or
    /// name@version). Returns number of rows removed. `mgc trust prune`.
    pub fn prune_trust_policies(&self) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM trust_policy WHERE package_id NOT IN (
                 SELECT id || '@' || version FROM packages
             ) AND package_id NOT IN (
                 SELECT id FROM packages
             )",
            [],
        )?;
        Ok(n)
    }

    /// Min-release-age (seconds) for an ecosystem. None = not set (use default).
    pub fn release_policy(&self, ecosystem: &str) -> Result<Option<u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT min_age_secs FROM release_policy WHERE ecosystem = ?1")?;
        let mut rows = stmt.query_map(params![ecosystem], |row| row.get::<_, u64>(0))?;
        match rows.next() {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn upsert_release_policy(&self, ecosystem: &str, min_age_secs: u64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO release_policy (ecosystem, min_age_secs) VALUES (?1, ?2)",
            params![ecosystem, min_age_secs],
        )?;
        Ok(())
    }

    /// Highest installed version for a package name — None = not installed.
    /// Used by the T5 no-downgrade guard (semver compare, not string compare).
    pub fn latest_installed_version(&self, name: &str) -> Result<Option<Version>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT version FROM packages WHERE id = ?1")?;
        let rows = stmt.query_map(params![name], |row| row.get::<_, String>(0))?;
        let mut best: Option<Version> = None;
        for row in rows {
            let v = match Version::parse(&row?) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if best.as_ref().is_none_or(|b| v > *b) {
                best = Some(v);
            }
        }
        Ok(best)
    }

    // ── Generation guard + project install lock (Gate 11-A, vòng-11) ────
    // The guard struct + its impls live at MODULE level below; this impl
    // exposes the two constructors that tie them to this Database.
    // (Guard struct + impl của nó nằm ở MODULE level bên dưới; impl này
    // expose 2 constructor gắn chúng với Database này.)

    /// Begin a staging generation and return it guarded by RAII: any drop
    /// without `disarm()` aborts the token (Gate 11-A item 6). The guard
    /// needs the DB PATH (not a borrow) so it can survive across awaits
    /// in async callers.
    /// (Begin staging generation và trả kèm guard RAII: drop nào không
    /// `disarm()` sẽ hủy token (Gate 11-A mục 6). Guard cần PATH DB
    /// (không phải borrow) để sống qua các await trong caller async.)
    pub fn begin_cas_generation_guarded(
        &self,
        db_path: &Path,
        project_root: &str,
    ) -> Result<CasGenerationGuard> {
        let generation = self.begin_cas_generation(project_root)?;
        Ok(CasGenerationGuard {
            db_path: db_path.to_path_buf(),
            project_root: project_root.to_string(),
            generation,
            armed: true,
        })
    }

    /// Project-level install lock (Gate 11-A, item 7 — vòng-11 verdict):
    /// one install per project at a time. SQLite generation tokens keep
    /// CAS claims crash-safe, but two installs mutating the SAME
    /// node_modules/lockfile in parallel is a correctness hazard the
    /// refcount protocol cannot fix (whoever renames last wins the
    /// tree). Production package managers serialize project mutation the
    /// same way. The lock is an exclusive OS file lock (fs2) held for
    /// the guard's lifetime — cross-process safe; a crashed process
    /// releases it automatically (OS closes the fd).
    /// (Khóa install mức project: một install mỗi project tại một thời
    /// điểm. Token generation SQLite giữ claim CAS an toàn khi crash,
    /// nhưng 2 install song song mutate cùng node_modules/lockfile là mối
    /// nguy correctness mà giao thức refcount không thể sửa (ai rename
    /// sau cùng thắng cả cây). Package manager production tuần tự hóa
    /// mutation project theo cách này. Khóa là file lock độc quyền OS
    /// (fs2) giữ suốt đời guard — an toàn cross-process; tiến trình đứt
    /// tự nhả (OS đóng fd).)
    pub fn project_install_lock(
        &self,
        project_root: &str,
    ) -> std::result::Result<ProjectInstallLock, CasGenerationError> {
        ProjectInstallLock::acquire(project_root)
    }
}

/// RAII generation guard (Gate 11-A, item 6): begin → claims → promote.
/// On ANY early return or panic, Drop aborts the STILL-STAGING token —
/// a crashed install can never leak a staging generation whose claims
/// over-protect blobs forever. `disarm()` after a successful promote
/// cancels the auto-abort; the guard is then a no-op.
///
/// The guard deliberately holds NO borrow of the Database (its caller
/// flows through async code where a borrowed guard crossing an await
/// makes the future !Send): Drop opens its OWN short-lived connection to
/// the same store DB and aborts there. Semantics match crash recovery —
/// if the process dies before Drop, the staging generation leaks to the
/// doctor's stale-staging GC, which is the same cleanup path.
/// (Guard generation RAII: begin → claim → promote. Mọi return sớm hay
/// panic, Drop hủy token VẪN-STAGING — install đứt không bao giờ rò
/// staging generation mà claim của nó giữ blob vô hạn. `disarm()` sau
/// promote thành công hủy auto-abort; guard khi đó là no-op.
/// Guard CỐ Ý không giữ borrow của Database (caller chảy qua async,
/// guard giữ borrow vượt await làm future !Send): Drop mở connection
/// RIÊNG tạm thời đến cùng store DB và hủy ở đó. Ngữ nghĩa khớp phục
/// hồi crash — process chết trước Drop thì staging generation rò tới GC
/// stale-staging của doctor, cùng đường dọn.)
pub struct CasGenerationGuard {
    db_path: std::path::PathBuf,
    project_root: String,
    generation: i64,
    armed: bool,
}

impl std::fmt::Debug for CasGenerationGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CasGenerationGuard")
            .field("project_root", &self.project_root)
            .field("generation", &self.generation)
            .field("armed", &self.armed)
            .finish()
    }
}

impl Drop for CasGenerationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // Fail-closed cleanup: a failed abort leaves the staging token to
        // the doctor's stale-staging GC — warned, never panicked (the guard
        // runs on drop paths). English-only console (RULE §7).
        // (Dọn fail-closed: abort thất bại để lại token staging cho GC
        // stale-staging của doctor — cảnh báo, không panic (guard chạy trên
        // đường drop). Console tiếng Anh (RULE §7).)
        match Database::open(&self.db_path)
            .map(|db| db.abort_cas_generation(&self.project_root, self.generation))
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!(
                "warning: failed to auto-abort cas generation {} for '{}': {e} \
                 (stale staging claims remain until doctor)",
                self.generation, self.project_root
            ),
            Err(e) => eprintln!(
                "warning: failed to open store db '{}' for auto-abort of cas \
                 generation {}: {e} (stale staging claims remain until doctor)",
                self.db_path.display(),
                self.generation
            ),
        }
    }
}

impl CasGenerationGuard {
    /// Disarm after a successful promote — the token is promoted, its
    /// claims are live; drop must NOT abort it.
    /// (Disarm sau promote thành công — token đã promoted, claim của
    /// nó live; drop KHÔNG được hủy nó.)
    pub fn disarm(mut self) {
        self.armed = false;
    }

    pub fn generation(&self) -> i64 {
        self.generation
    }
}

/// Project-level exclusive install lock (Gate 11-A item 7). File:
/// `<store>/locks/install-<digest>.lock` — digest = blake3 of the
/// canonical project root, so every spelling of one project maps to one
/// lock file. ACQUISITION CONTRACT (P3 comment fix, fresh-context
/// review 2026-09-15): this is a TRY-lock — a second concurrent install
/// FAILS FAST with a typed error (the old doc claimed "blocks (with
/// retry)", which the code never did; the honest contract is fail-fast,
/// matching production package managers' install-lock UX).
/// (Khóa install độc quyền mức project. File:
/// `<store>/locks/install-<digest>.lock` — digest = blake3 của project
/// root chuẩn hóa, mọi cách viết một project ánh xạ một file lock. HỢP
/// ĐỒNG LẤY KHÓA: đây là TRY-lock — install song song thứ hai FAIL NGAY
/// với lỗi có type (doc cũ claim "chặn (kèm retry)" trong khi code
/// chưa bao giờ làm vậy; hợp đồng trung thực là fail-fast, khớp UX
/// install-lock của package manager production.)
pub struct ProjectInstallLock {
    file: std::fs::File,
    path: std::path::PathBuf,
}

impl ProjectInstallLock {
    /// Lock file root — the store's locks/ directory (Layout::locks_dir).
    /// RULE §12 (no hardcode): the base honors MAGICORE_STORE_ROOT /
    /// default_store_root().
    /// (Root của file lock — thư mục locks/ của store (Layout::locks_dir).
    /// RULE §12: base tôn trọng MAGICORE_STORE_ROOT / default_store_root().)
    fn locks_root() -> std::path::PathBuf {
        crate::default_store_root().join("locks")
    }

    /// Acquire against the DEFAULT store root (production path).
    /// (Lấy khóa theo store root MẶC ĐỊNH (đường production).)
    fn acquire(project_root: &str) -> std::result::Result<Self, CasGenerationError> {
        Self::acquire_at(&Self::locks_root(), project_root)
    }

    /// Acquire against an explicit locks root — the seam that keeps
    /// tests hermetic (no global env mutation, no user-store touch) and
    /// lets embedders place the lock elsewhere.
    /// (Lấy khóa theo root locks tường minh — đường giữ test kín (không
    /// đổi env toàn cục, không chạm store user) và cho embedder đặt
    /// lock ở nơi khác.)
    pub fn acquire_at(
        root: &std::path::Path,
        project_root: &str,
    ) -> std::result::Result<Self, CasGenerationError> {
        Self::acquire_impl(root, project_root)
    }

    fn acquire_impl(
        root: &std::path::Path,
        project_root: &str,
    ) -> std::result::Result<Self, CasGenerationError> {
        std::fs::create_dir_all(root).map_err(|e| {
            CasGenerationError::Io(format!("create locks dir '{}': {e}", root.display()))
        })?;
        // Digest via the shared crypto crate — mgc-store does not depend on
        // blake3 directly (dependency discipline, no duplicate hashers).
        // (Digest qua crate crypto dùng chung — mgc-store không phụ thuộc
        // blake3 trực tiếp (kỷ luật dependency, không nhân đôi hasher).)
        let digest = mgc_crypto::Blake3Hasher::hash_string(project_root).to_hex();
        let path = root.join(format!("install-{digest}.lock"));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| CasGenerationError::Io(format!("open lock '{}': {e}", path.display())))?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|e| {
            CasGenerationError::Io(format!(
                "another install holds the project lock \
                 ('{}' — concurrent installs of one project are serialized): {e}",
                path.display()
            ))
        })?;
        Ok(Self { file, path })
    }
}

impl std::fmt::Debug for ProjectInstallLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectInstallLock")
            .field("path", &self.path)
            .finish()
    }
}

impl Drop for ProjectInstallLock {
    fn drop(&mut self) {
        fs2::FileExt::unlock(&self.file).ok();
        // The lock FILE stays — a zero-byte lock file is the standard
        // pattern (flock on the inode); removing it on drop re-opens the
        // rename race on POSIX.
        // (File lock GIỮ NGUYÊN — file lock 0-byte là pattern chuẩn (flock
        // trên inode); xóa nó lúc drop mở lại race rename trên POSIX.)
    }
}
