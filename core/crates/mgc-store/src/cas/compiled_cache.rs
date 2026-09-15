// Compiled-module cache keyed by a VERSIONED compilation key (P0-1/P0-2/P0-3
// audit vòng-4). The old design keyed entries by the source content hash
// only — the same bytes compiled as .ts vs .tsx, or by two compiler
// versions, collided onto one entry and could serve WRONG JavaScript to
// another project. The new design:
//
// P0-1 — `CompilationKey` carries the FULL compilation context: schema
// version, source digest, loader, compiler identity+version, and a digest of
// the canonical options JSON. The cache path is derived from the BLAKE3 of
// the canonical key JSON, so every distinct compilation context gets a
// distinct entry.
//
// P0-2 — race contract is now DETERMINISTIC on every OS: publication uses
// `hard_link` (fails with AlreadyExists when the entry exists on both Unix
// and Windows — first writer wins, no rename-overwrite last-wins). A loser
// NEVER silently reports success: it reads the winner, verifies it, and
// either dedups (identical output digest → Ok), replaces it (stale/corrupt
// winner → quarantine/remove + retry link once), or fails with a typed
// `CacheConflict` (divergent output for the same key is nondeterministic
// compilation or tampering — never faked Ok).
//
// P0-3 — records are SELF-VALIDATING: every stored JSON embeds the
// schema version, the full compilation key, and the BLAKE3 digest of the
// output. `get()` re-verifies all three BEFORE serving; a well-formed JSON
// whose payload was swapped ("poisoned") fails the digest check, is moved to
// `quarantine/` for forensics, and returns a hard `CacheCorrupt` error —
// poison is never served to the dev server.
// THREAT MODEL (P0-4, Tech Lead vòng-6/7): this cache is self-validating
// against ACCIDENTAL CORRUPTION and INCOMPLETE WRITES only. It is NOT
// authenticated against a malicious writer running as the SAME user: such
// a process can already rewrite binaries, configs and project sources, and
// can recompute every digest in a forged record. Remote/untrusted caches
// would need signatures, MAC keys or provenance by namespace — out of
// scope for the local store by design.
//
// Cache module đã biên dịch, khóa theo compilation key CÓ PHIÊN BẢN (P0-1/
// P0-2/P0-3 audit vòng-4). Thiết kế cũ chỉ khóa theo hash nội dung source —
// cùng bytes compile theo .ts với .tsx, hoặc bằng 2 phiên bản compiler, bị
// dồn vào 1 entry và có thể phục vụ JavaScript SAI cho project khác. Thiết
// kế mới:
//
// P0-1 — `CompilationKey` mang ĐẦY ĐỦ ngữ cảnh biên dịch: schema version,
// digest source, loader, identity+version compiler, và digest của JSON
// options chuẩn hóa. Path cache suy ra từ BLAKE3 của JSON key chuẩn hóa —
// mọi ngữ cảnh biên dịch khác nhau có entry khác nhau.
//
// P0-2 — hợp đồng race giờ TẤT ĐỊNH trên mọi OS: publish dùng `hard_link`
// (thất bại AlreadyExists khi entry tồn tại trên cả Unix lẫn Windows —
// writer đầu thắng, không có last-wins bằng rename-đè). Bên thua KHÔNG BAO
// GIỜ báo thành công im lặng: đọc winner, verify winner, rồi hoặc dedup
// (digest output giống nhau → Ok), hoặc thay thế (winner stale/hỏng →
// quarantine/remove + retry link đúng 1 lần), hoặc fail với lỗi typed
// `CacheConflict` (output phân kỳ cùng key là biên dịch không tất định
// hoặc bị can thiệp — không bao giờ giả Ok).
//
// P0-3 — record TỰ KIỂM TRA VỚI CORRUPTION NGẪU NHIÊN (self-validating,
// KHÔNG phải self-authenticating): mọi JSON lưu nhúng schema version,
// compilation key đầy đủ, và digest BLAKE3 của output. `get()` verify lại
// cả ba TRƯỚC khi phục vụ; JSON hợp lệ nhưng payload bị đổi ("đầu độc")
// rơi khỏi check digest, bị chuyển vào `quarantine/` để điều tra, và trả
// lỗi cứng `CacheCorrupt` — độc không bao giờ tới dev server.
// THREAT MODEL (P0-4): cache này tự KIỂM TRA (self-validating, chống hỏng
// ngẫu nhiên + ghi đứt giữa chừng), KHÔNG TỰ XÁC THỰC (self-authenticating)
// chống writer ác ý cùng user — process đó đã có thể sửa binary, config,
// source project, và recompute mọi digest trong record giả mạo. Cache
// remote/không tin cậy cần chữ ký, khóa MAC hay provenance theo namespace —
// ngoài scope của store local theo thiết kế.

use super::integrity::{IntegrityHash, validate_blake3_hex};
use super::store::StoreError;
use super::write::{sync_parent_dir, unique_tmp_path};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Current on-disk record schema. Bump when the record layout changes —
/// entries written by an older schema are treated as STALE (cache miss →
/// recompile → replace), never served.
///
/// v2 (P1-1, Tech Lead vòng-5/6/7): output digest switched from the
/// ambiguous `js || 0x00 || sm` concatenation (where `None ≡ Some("")` and
/// NUL bytes inside payloads created collisions) to a domain-separated
/// length-prefixed encoding.
///
/// Schema record trên đĩa hiện tại. Tăng khi layout record đổi — entry ghi
/// bởi schema cũ được coi là STALE (cache miss → biên dịch lại → thay),
/// không bao giờ phục vụ.
///
/// v2 (P1-1): digest output đổi từ nối chuỗi mơ hồ `js || 0x00 || sm`
/// (trong đó `None ≡ Some("")` và byte NUL trong payload tạo collision)
/// sang mã hóa length-prefixed tách miền.
pub const COMPILED_CACHE_SCHEMA_VERSION: u32 = 2;

/// Loader semantics for a compilation key (P0-1): the same text compiles
/// differently per loader, so the loader is part of the key's identity —
/// and it must be a CLOSED SET, not a free-form String ("garbage-loader"
/// used to pass validation and fork the cache namespace).
///
/// Semantics loader cho compilation key (P0-1): cùng text biên dịch khác
/// nhau theo loader nên loader là một phần identity của key — và nó phải
/// là tập ĐÓNG, không phải String tự do ("garbage-loader" từng lọt qua
/// validation và rẽ nhánh namespace cache).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Loader {
    Ts,
    Tsx,
    Jsx,
    Js,
    Mjs,
}

impl Loader {
    /// Map a file extension to its loader — the ONLY sanctioned way the
    /// dev server derives loader identity.
    /// (Ánh xạ đuôi file sang loader — cách DUY NHẤT được phép để dev
    /// server suy ra identity loader.)
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "ts" => Some(Self::Ts),
            "tsx" => Some(Self::Tsx),
            "jsx" => Some(Self::Jsx),
            "js" => Some(Self::Js),
            "mjs" => Some(Self::Mjs),
            _ => None,
        }
    }
}

impl std::fmt::Display for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ts => "ts",
            Self::Tsx => "tsx",
            Self::Jsx => "jsx",
            Self::Js => "js",
            Self::Mjs => "mjs",
        })
    }
}

impl std::str::FromStr for Loader {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_extension(s)
            .ok_or_else(|| format!("unknown loader '{s}' (expected ts|tsx|jsx|js|mjs)"))
    }
}

/// Serde via the Display/FromStr pair: records carry the lowercase tag,
/// and a record whose loader is NOT in the closed set fails to parse —
/// a garbage-loader record can never become a CompilationKey again.
/// (Serde qua cặp Display/FromStr: record mang tag lowercase, record có
/// loader NGOÀI tập đóng thì parse fail — record garbage-loader không
/// bao giờ thành CompilationKey được nữa.)
impl serde::Serialize for Loader {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Loader {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<Self>().map_err(serde::de::Error::custom)
    }
}

/// Versioned identity of ONE compilation (P0-1): everything the output
/// depends on must be part of the key, or two different compilations can
/// collide onto one cache entry.
///
/// INVARIANT (P0-1, Tech Lead vòng-5/6/7): ALL fields are PRIVATE. The
/// only construction path outside the crate is `CompilationKey::new`,
/// which validates every input; deserialization goes through the same
/// validation (`validate()`), so a hand-crafted record JSON with a junk
/// key can NEVER occupy a cache address. Struct-literal bypass and field
/// tampering are compile-time errors (see tests/compile_fail.rs).
///
/// Identity CÓ PHIÊN BẢN của MỘT lần biên dịch (P0-1): mọi thứ mà output
/// phụ thuộc vào phải nằm trong key, nếu không 2 lần biên dịch khác nhau có
/// thể đụng nhau trên 1 entry cache.
///
/// INVARIANT (P0-1): MỌI field là PRIVATE. Đường dựng duy nhất ngoài crate
/// là `CompilationKey::new` — validate mọi đầu vào; deserialize đi qua
/// cùng bước validate (`validate()`) nên record JSON tự chế với key rác
/// KHÔNG BAO GIỜ chiếm được địa chỉ cache. Struct-literal bypass và sửa
/// field trực tiếp là lỗi compile-time (xem tests/compile_fail.rs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompilationKey {
    /// On-disk schema of the cache entry layout.
    /// Schema layout của entry cache trên đĩa.
    schema_version: u32,
    /// BLAKE3 hex of the source bytes (the ONLY content input).
    /// BLAKE3 hex của bytes source (đầu vào nội dung DUY NHẤT).
    source_digest: String,
    /// Loader semantics: the same text compiles differently per loader.
    /// Semantics loader: cùng text biên dịch khác nhau theo loader.
    loader: Loader,
    /// Compiler identity (e.g. "esbuild-rs").
    /// Danh tính compiler (vd "esbuild-rs").
    compiler: String,
    /// Compiler version — outputs of two compiler versions never share an
    /// entry.
    /// Phiên bản compiler — output của 2 phiên bản compiler không bao giờ
    /// dùng chung entry.
    compiler_version: String,
    /// BLAKE3 hex of the canonical compile-options JSON (platform, target,
    /// format, jsx mode, sourcemap, minify, define, ...).
    /// BLAKE3 hex của JSON options biên dịch chuẩn hóa (platform, target,
    /// format, jsx mode, sourcemap, minify, define, ...).
    options_digest: String,
}

impl CompilationKey {
    /// Build a validated compilation key. `options_json` must be a valid
    /// JSON object produced canonically by the caller (sorted keys) — its
    /// digest is embedded, the raw string is not stored.
    ///
    /// Dựng compilation key đã validate. `options_json` phải là JSON object
    /// hợp lệ do caller sinh chuẩn hóa (key đã sort) — digest của nó được
    /// nhúng, chuỗi thô không lưu.
    pub fn new(
        source_digest: &str,
        loader: Loader,
        compiler: &str,
        compiler_version: &str,
        options_json: &str,
    ) -> Result<Self, StoreError> {
        // Fail-closed digest validation — the source digest must be a real
        // 64-hex BLAKE3 value before it can address a cache entry.
        // (Validate digest fail-closed — digest source phải là BLAKE3 hex
        // 64 thật trước khi được làm địa chỉ entry cache.)
        validate_blake3_hex(source_digest).map_err(|e| StoreError::InvalidHash(e.input))?;
        if compiler.is_empty() || compiler_version.is_empty() {
            return Err(StoreError::InvalidHash(format!(
                "compilation key requires non-empty compiler/version (compiler='{compiler}')"
            )));
        }
        // The options must be well-formed JSON — a malformed string here is
        // a caller bug, fail loudly instead of hashing garbage.
        // (Options phải là JSON hợp lệ — chuỗi sai dạng là bug của caller,
        // hét to thay vì hash rác.)
        serde_json::from_str::<serde_json::Value>(options_json).map_err(|e| StoreError::Io {
            path: PathBuf::from("<compilation-options>"),
            msg: format!("options_json is not valid JSON: {e}"),
        })?;
        let options_digest = IntegrityHash::from_bytes(options_json.as_bytes(), false)
            .as_hex()
            .to_string();
        Ok(Self {
            schema_version: COMPILED_CACHE_SCHEMA_VERSION,
            source_digest: source_digest.to_string(),
            loader,
            compiler: compiler.to_string(),
            compiler_version: compiler_version.to_string(),
            options_digest,
        })
    }

    /// Re-validate EVERY field of a parsed key (P0-1): a key that came
    /// from a record's JSON must satisfy the same invariants as one built
    /// by `new()` — digests are real BLAKE3 hex, loader is in the closed
    /// set (already enforced by the `Loader` deserializer), and the
    /// identity fields are non-empty. Records failing this are corrupt,
    /// never served.
    ///
    /// Kiểm tra lại MỌI field của key đã parse (P0-1): key đến từ JSON
    /// record phải thỏa cùng invariant như key dựng bằng `new()` — digest
    /// là BLAKE3 hex thật, loader thuộc tập đóng (deserializer `Loader` đã
    /// cưỡng chế), và các field identity khác rỗng. Record fail chỗ này là
    /// hỏng, không bao giờ phục vụ.
    fn validate(&self) -> Result<(), StoreError> {
        validate_blake3_hex(&self.source_digest).map_err(|e| StoreError::InvalidHash(e.input))?;
        validate_blake3_hex(&self.options_digest).map_err(|e| StoreError::InvalidHash(e.input))?;
        if self.compiler.is_empty() || self.compiler_version.is_empty() {
            return Err(StoreError::InvalidHash(
                "compilation key has empty compiler identity".to_string(),
            ));
        }
        Ok(())
    }

    // ── Read-only accessors (P0-1): the fields are private; these are
    // the only way outside code can READ the key — no mutation path.
    // (Accessor chỉ-đọc (P0-1): field private; đây là cách duy nhất code
    // ngoài ĐỌC key — không có đường sửa.)

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn loader(&self) -> Loader {
        self.loader
    }

    pub fn compiler(&self) -> &str {
        &self.compiler
    }

    pub fn compiler_version(&self) -> &str {
        &self.compiler_version
    }

    pub fn options_digest(&self) -> &str {
        &self.options_digest
    }

    /// Canonical serialized form (serde struct field order is fixed →
    /// deterministic byte-for-byte for the same key).
    /// (Dạng serialize chuẩn hóa — thứ tự field struct serde cố định → tất
    /// định từng byte cho cùng key.)
    fn canonical(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Cache address for this key: BLAKE3 of the canonical key JSON. Two
    /// keys differing in ANY field → different address (the P0-1 collision
    /// is structurally impossible).
    ///
    /// Địa chỉ cache cho key này: BLAKE3 của JSON key chuẩn hóa. 2 key khác
    /// nhau ở BẤT KỲ field nào → địa chỉ khác (collision P0-1 về cấu trúc
    /// không thể xảy ra).
    pub fn master_digest(&self) -> IntegrityHash {
        IntegrityHash::from_bytes(self.canonical().as_bytes(), false)
    }
}

/// Deserialization contract (P0-1): a key parsed from record JSON goes
/// through `Loader`'s closed-set deserializer AND `validate()` — junk
/// digests or empty identity fields make the WHOLE record fail to parse,
/// which `get()` treats as corrupt (quarantine + CacheCorrupt), never a
/// serve.
///
/// Hợp đồng deserialize (P0-1): key parse từ record JSON đi qua
/// deserializer tập đóng của `Loader` VÀ `validate()` — digest rác hay
/// field identity rỗng làm CẢ record fail parse, và `get()` coi là hỏng
/// (quarantine + CacheCorrupt), không bao giờ phục vụ.
impl<'de> serde::Deserialize<'de> for CompilationKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Wire shape mirroring the private fields (parse-then-validate).
        /// (Shape wire phản chiếu field private (parse-rồi-validate).)
        #[derive(serde::Deserialize)]
        struct CompilationKeyWire {
            schema_version: u32,
            source_digest: String,
            loader: Loader,
            compiler: String,
            compiler_version: String,
            options_digest: String,
        }
        let wire = CompilationKeyWire::deserialize(deserializer)?;
        let key = Self {
            schema_version: wire.schema_version,
            source_digest: wire.source_digest,
            loader: wire.loader,
            compiler: wire.compiler,
            compiler_version: wire.compiler_version,
            options_digest: wire.options_digest,
        };
        key.validate()
            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
        Ok(key)
    }
}

/// Compiled output payload (what callers put in and get back).
/// (Payload output đã biên dịch — thứ caller đưa vào và nhận lại.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledModule {
    pub js: String,
    pub source_map: Option<String>,
}

/// Outcome of record self-validation (P0-3) — typed, not stringly.
/// (Kết quả tự kiểm tra record (P0-3) — typed, không chuỗi hóa.)
#[derive(Debug, Clone, PartialEq, Eq)]
enum VerifyOutcome {
    /// Record is authentic for this key — safe to serve.
    /// (Record xác thực đúng key này — an toàn để phục vụ.)
    Authentic,
    /// Record was written by an older schema — a legit old entry, NOT
    /// corruption: report a miss so the caller recompiles and replaces it.
    /// (Record do schema cũ ghi — entry hợp lệ của thời trước, KHÔNG phải
    /// hỏng: báo miss để caller biên dịch lại và thay nó.)
    StaleSchema,
    /// Verification failed (key/digest mismatch) — poison or tampering.
    /// Carries the human-readable reason for the forensic report.
    /// (Verify fail (lệch key/digest) — độc hoặc gian lận. Mang lý do đọc
    /// được cho báo cáo điều tra.)
    Failed(String),
}

/// Self-validating on-disk record (P0-3): payload + the digest that
/// proves the payload was produced for exactly this key by exactly this
/// compiler. `get()` refuses to serve anything that fails verification.
/// NOT self-AUTHENTICATING: the digest is unkeyed BLAKE3 — it defends
/// against accidental corruption and torn writes, NOT against a
/// same-user malicious writer (see the threat-model header at the top).
///
/// Record trên đĩa TỰ KIỂM TRA (P0-3): payload + digest chứng minh payload
/// được sinh cho đúng key này bởi đúng compiler này. `get()` từ chối phục
/// vụ bất cứ gì rơi khỏi bước verify. KHÔNG phải tự XÁC THỰC: digest là
/// BLAKE3 không khóa — chống hỏng ngẫu nhiên và ghi đứt, KHÔNG chống
/// writer cùng user cố tình (xem threat-model đầu file).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompiledRecord {
    /// `#[serde(default)]`: records from an older schema (or hand-crafted
    /// garbage) parse with 0 → treated as STALE, reported as a miss and
    /// replaced — never served.
    /// (`#[serde(default)]`: record từ schema cũ (hoặc rác cố tình) parse
    /// ra 0 → coi là STALE, báo miss và bị thay — không bao giờ phục vụ.)
    #[serde(default)]
    schema_version: u32,
    key: CompilationKey,
    /// BLAKE3 hex over `js || 0x00 || source_map.unwrap_or("")` — the full
    /// output identity.
    /// BLAKE3 hex trên `js || 0x00 || source_map.unwrap_or("")` — toàn bộ
    /// identity của output.
    output_digest: String,
    js: String,
    source_map: Option<String>,
}

impl CompiledRecord {
    fn from_module(key: &CompilationKey, module: &CompiledModule) -> Self {
        let output_digest = output_digest_of(&module.js, module.source_map.as_deref());
        Self {
            schema_version: COMPILED_CACHE_SCHEMA_VERSION,
            key: key.clone(),
            output_digest,
            js: module.js.clone(),
            source_map: module.source_map.clone(),
        }
    }

    fn to_module(&self) -> CompiledModule {
        CompiledModule {
            js: self.js.clone(),
            source_map: self.source_map.clone(),
        }
    }

    /// Self-validation check (P0-3): embedded schema + key equality +
    /// output digest must ALL hold before this record may be served.
    ///
    /// Kiểm tra tự kiểm tra (P0-3): schema nhúng + bằng nhau key + digest
    /// output phải ĐỦ điều kiện trước khi record này được phục vụ.
    fn verify_against(&self, expected_key: &CompilationKey) -> VerifyOutcome {
        if self.schema_version != COMPILED_CACHE_SCHEMA_VERSION {
            return VerifyOutcome::StaleSchema;
        }
        if &self.key != expected_key {
            return VerifyOutcome::Failed(format!(
                "key mismatch: entry holds loader '{}' compiler '{}'/'{}', requested loader '{}' compiler '{}'/'{}'",
                self.key.loader,
                self.key.compiler,
                self.key.compiler_version,
                expected_key.loader,
                expected_key.compiler,
                expected_key.compiler_version
            ));
        }
        if output_digest_of(&self.js, self.source_map.as_deref()) != self.output_digest {
            return VerifyOutcome::Failed("output digest mismatch (poisoned payload)".to_string());
        }
        VerifyOutcome::Authentic
    }
}

/// Domain-separated output identity (P1-1, schema v2): BLAKE3 over
/// `MGC-CC-OV2 ‖ u64-LE len(js) ‖ js ‖ option-tag ‖ u64-LE len(sm) ‖ sm`
/// where option-tag is `0x00` for `None` and `0x01` for `Some`. The old
/// `js || 0x00 || sm` encoding had TWO collisions: (1) `source_map: None`
/// hashed identically to `source_map: Some("")`; (2) a NUL byte inside
/// `js`/`sm` made different (js, sm) splits hash the same. Length
/// prefixes and the presence tag make the encoding prefix-free —
/// injective by construction: same digest ⟺ same module.
///
/// Identity của output tách miền (P1-1, schema v2): BLAKE3 trên
/// `MGC-CC-OV2 ‖ u64-LE len(js) ‖ js ‖ option-tag ‖ u64-LE len(sm) ‖ sm`
/// với option-tag là `0x00` cho `None` và `0x01` cho `Some`. Mã hóa cũ
/// `js || 0x00 || sm` có HAI collision: (1) `source_map: None` hash giống
/// hệt `source_map: Some("")`; (2) byte NUL trong `js`/`sm` khiến cách
/// tách (js, sm) khác nhau hash ra giống nhau. Prefix độ dài + tag hiện
/// diện khiến mã hóa prefix-free — đơn ánh theo cấu trúc: cùng digest ⟺
/// cùng module.
fn output_digest_of(js: &str, source_map: Option<&str>) -> String {
    const SCHEMA_TAG: &[u8] = b"MGC-CC-OV2";
    let mut hasher = blake3::Hasher::new();
    hasher.update(SCHEMA_TAG);
    hasher.update(&(js.len() as u64).to_le_bytes());
    hasher.update(js.as_bytes());
    match source_map {
        None => {
            hasher.update(&[0x00]);
        }
        Some(sm) => {
            hasher.update(&[0x01]);
            hasher.update(&(sm.len() as u64).to_le_bytes());
            hasher.update(sm.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

pub struct CompiledCache {
    root: PathBuf,
}

impl CompiledCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Cache entry path for a key (sharded 2-hex, same layout as the CAS).
    /// (Path entry cache cho 1 key (shard 2-hex, cùng layout với CAS).)
    fn module_path(&self, key: &CompilationKey) -> PathBuf {
        let master = key.master_digest();
        let algo_dir = self.root.join("compiled").join("blake3");
        // Safe accessor: IntegrityHash type guarantees 64 hex chars (P0-1).
        // (Accessor an toàn: type IntegrityHash đảm bảo 64 ký tự hex (P0-1).)
        let hex = master.as_hex();
        let first2 = &hex[..2];
        algo_dir.join(first2).join(hex).with_extension("json")
    }

    /// Read + verify a compiled entry (P0-3). Returns:
    /// - `Ok(Some(module))` — entry exists and PASSED self-validation;
    /// - `Ok(None)` — miss, or STALE schema (old-format entry: recompile
    ///   and `put` replaces it);
    /// - `Err(CacheCorrupt)` — well-formed-looking entry whose
    ///   key/digest verification FAILED (poison/tamper): quarantined for
    ///   forensics, never served.
    ///
    /// Đọc + verify entry đã biên dịch (P0-3). Trả về:
    /// - `Ok(Some(module))` — entry tồn tại và ĐÃ qua tự kiểm tra;
    /// - `Ok(None)` — miss, hoặc schema STALE (entry cũ: biên dịch lại rồi
    ///   `put` sẽ thay);
    /// - `Err(CacheCorrupt)` — entry trông hợp lệ nhưng verify key/digest
    ///   THẤT BẠI (đầu độc/gian lận): bị cách ly để điều tra, không phục vụ.
    pub fn get(&self, key: &CompilationKey) -> Result<Option<CompiledModule>, StoreError> {
        let path = self.module_path(key);
        let Some(meta) = fs::symlink_metadata(&path).ok() else {
            return Ok(None);
        };
        if meta.file_type().is_symlink() {
            return Err(StoreError::CacheCorrupt {
                path,
                detail: "compiled cache entry is a symlink".to_string(),
            });
        }

        let data = fs::read(&path)?;
        let record: CompiledRecord = match serde_json::from_slice(&data) {
            Ok(r) => r,
            // Torn/unparseable bytes — NOT a serve-able entry. Quarantine
            // for forensics and fail closed (P0-3): the dev server treats
            // the error as a miss and recompiles; its next put replaces
            // the quarantined slot.
            // (Bytes đứt/không parse được — KHÔNG phải entry phục vụ được.
            // Cách ly để điều tra và fail cứng (P0-3): dev server coi lỗi
            // như miss và biên dịch lại; put kế tiếp thay slot đã cách ly.)
            Err(e) => {
                let detail = format!("failed to parse compiled record: {e}");
                self.quarantine_corrupt(&path, key, &detail)?;
                return Err(StoreError::CacheCorrupt { path, detail });
            }
        };

        match record.verify_against(key) {
            VerifyOutcome::Authentic => Ok(Some(record.to_module())),
            VerifyOutcome::StaleSchema => {
                // Old-format entry: NOT corruption, report a miss so the
                // caller recompiles; the following put replaces it.
                // (Entry format cũ: KHÔNG phải hỏng, báo miss để caller
                // biên dịch lại; put kế tiếp sẽ thay nó.)
                Ok(None)
            }
            VerifyOutcome::Failed(reason) => {
                // Verification failed — poison or tampering. Quarantine
                // for forensics and fail closed (P0-3).
                // (Verify fail — độc hoặc gian lận. Cách ly để điều tra và
                // fail cứng (P0-3).)
                self.quarantine_corrupt(&path, key, &reason)?;
                Err(StoreError::CacheCorrupt {
                    path,
                    detail: reason,
                })
            }
        }
    }

    /// Publish a compiled output atomically and deterministically (P0-2).
    ///
    /// Race contract (identical on Unix and Windows — publication uses
    /// `hard_link`, which fails when the destination exists on BOTH):
    /// 1. Serialize the self-validating record → unique temp
    ///    (create_new) → write → fsync.
    /// 2. `hard_link(temp → entry)`: the FIRST complete writer wins; a
    ///    loser never overwrites the winner (no last-wins nondeterminism).
    /// 3. On AlreadyExists the loser READS and VERIFIES the winner:
    ///    - identical output digest → Ok (legit dedup);
    ///    - stale/corrupt winner → quarantine/remove + retry link once;
    ///    - divergent payload, same key → typed `CacheConflict` — the API
    ///      never fakes success for a divergent result.
    ///
    /// Publish output đã biên dịch nguyên tử và tất định (P0-2).
    ///
    /// Hợp đồng race (giống hệt trên Unix và Windows — publish dùng
    /// `hard_link`, lỗi khi đích tồn tại trên CẢ HAI nền tảng):
    /// 1. Serialize record tự kiểm tra (self-validating) → temp duy nhất
    ///    (create_new) → ghi → fsync.
    /// 2. `hard_link(temp → entry)`: writer HOÀN CHỈNH ĐẦU TIÊN thắng; bên
    ///    thua không bao giờ đè winner (không có last-wins không tất định).
    /// 3. Khi AlreadyExists, bên thua ĐỌC và VERIFY winner:
    ///    - digest output giống nhau → Ok (dedup hợp lệ);
    ///    - winner stale/hỏng → quarantine/remove + retry link đúng 1 lần;
    ///    - payload phân kỳ cùng key → `CacheConflict` typed — API không
    ///      bao giờ giả thành công cho kết quả phân kỳ.
    pub fn put(&self, key: &CompilationKey, module: &CompiledModule) -> Result<(), StoreError> {
        let path = self.module_path(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let record = CompiledRecord::from_module(key, module);
        let data = serde_json::to_vec(&record).map_err(|e| StoreError::Io {
            path: path.clone(),
            msg: format!("failed to serialize compiled record: {e}"),
        })?;

        let parent = path.parent().unwrap_or(&self.root);
        let tmp = unique_tmp_path(parent, "compiled");
        let mut guard = TmpGuard::new(&tmp);

        let mut file = fs::File::create_new(&tmp)?;
        std::io::Write::write_all(&mut file, &data)?;
        std::io::Write::flush(&mut file)?;
        let sync = file.sync_all();
        drop(file);
        sync?;

        match fs::hard_link(&tmp, &path) {
            Ok(()) => {
                // First writer: the link IS the publish — drop our temp name
                // and make the link durable. REMOVE FIRST, disarm SECOND
                // (P1-4, Tech Lead vòng-6/7): the old order disarmed the
                // RAII guard before removing, so a failed remove left an
                // ORPHAN temp forever (no retry possible). Keeping the
                // guard armed through the remove means a failure still
                // retries via Drop — and a success path that cannot
                // remove its own just-written temp is a real filesystem
                // problem worth surfacing, not swallowing.
                // (Writer đầu: link chính là publish — bỏ tên temp và
                // khiến link bền vững. REMOVE TRƯỚC, disarm SAU (P1-4):
                // thứ tự cũ disarm guard RAII trước khi remove nên remove
                // fail để lại temp MỒ CÔI vĩnh viễn (không retry được).
                // Giữ guard armed qua remove nghĩa là fail vẫn retry qua
                // Drop — còn đường thành công mà không xóa được temp do
                // chính mình vừa ghi là lỗi filesystem thật đáng báo, không
                // đáng nuốt.)
                match fs::remove_file(&tmp) {
                    Ok(()) => {
                        guard.disarm();
                        sync_parent_dir(parent)?;
                        Ok(())
                    }
                    Err(e) => Err(StoreError::Io {
                        path: tmp.to_path_buf(),
                        msg: format!("published the entry but failed to remove its temp: {e}"),
                    }),
                }
            }
            Err(e) if path.exists() => {
                // Lost the publication race — the winner exists. Verify it
                // before deciding anything (P0-2: no silent Ok).
                // (Thua race publish — winner đã có. Verify winner trước khi
                // quyết định bất cứ gì (P0-2: không Ok im lặng).)
                let _ = e;
                self.resolve_race(key, &path, &tmp, &record)
            }
            Err(e) => Err(StoreError::Io {
                path: path.clone(),
                msg: format!("atomic compiled-cache publish (hard_link) failed: {e}"),
            }),
        }
    }

    /// Decide the outcome after losing the publication race (P0-2).
    /// The winner is read, verified, and compared — never trusted blindly.
    ///
    /// Quyết định kết quả sau khi thua race publish (P0-2). Winner được
    /// đọc, verify, và so sánh — không bao giờ tin mù.
    fn resolve_race(
        &self,
        key: &CompilationKey,
        path: &Path,
        tmp: &Path,
        ours: &CompiledRecord,
    ) -> Result<(), StoreError> {
        let winner = match self.read_record(path) {
            Ok(w) => w,
            // Torn/unparseable winner bytes — quarantine whatever is there
            // and take over the slot with our fresh record (self-healing).
            // (Bytes winner đứt/không parse được — cách ly phần còn lại và
            // nhận slot bằng record mới (tự sửa).)
            Err(e) => {
                self.quarantine_corrupt(path, key, &format!("{e}"))?;
                return self.publish_retry(path, tmp);
            }
        };

        match winner.verify_against(key) {
            // Winner is valid AND identical → legitimate dedup.
            // (Winner hợp lệ VÀ giống hệt → dedup hợp lệ.)
            VerifyOutcome::Authentic if winner.output_digest == ours.output_digest => Ok(()),
            // Winner is valid but DIVERGENT — same key produced two
            // different outputs: nondeterministic compilation or tampering.
            // Never fake success; leave the valid winner in place.
            // (Winner hợp lệ nhưng PHÂN KỲ — cùng key sinh 2 output khác
            // nhau: biên dịch không tất định hoặc bị can thiệp. Không giả
            // thành công; giữ nguyên winner hợp lệ.)
            VerifyOutcome::Authentic => Err(StoreError::CacheConflict {
                path: path.to_path_buf(),
                expected: ours.output_digest.clone(),
                winner: winner.output_digest.clone(),
            }),
            // Winner is STALE (old schema) — remove it and publish the
            // fresh record in its place (exactly one retry).
            // (Winner STALE (schema cũ) — xóa nó và publish record mới vào
            // chỗ của nó (retry đúng 1 lần).)
            VerifyOutcome::StaleSchema => {
                fs::remove_file(path)?;
                self.publish_retry(path, tmp)
            }
            // Winner is CORRUPT — quarantine for forensics, then publish
            // the fresh record in its place (self-healing).
            // (Winner HỎNG — cách ly để điều tra, rồi publish record mới
            // vào chỗ của nó (tự sửa).)
            VerifyOutcome::Failed(reason) => {
                self.quarantine_corrupt(path, key, &reason)?;
                self.publish_retry(path, tmp)
            }
        }
    }

    /// Link the (already fsynced) temp onto the now-free entry path exactly
    /// once. A failure here surfaces as a conflict — never a silent Ok.
    ///
    /// Link temp (đã fsync sẵn) vào entry path vừa trống đúng 1 lần. Lỗi ở
    /// đây báo conflict — không bao giờ Ok im lặng.
    fn publish_retry(&self, path: &Path, tmp: &Path) -> Result<(), StoreError> {
        match fs::hard_link(tmp, path) {
            Ok(()) => {
                // Same P1-4 discipline as the first-writer arm: surface a
                // temp-remove failure instead of leaving an orphan.
                // (Cùng kỷ luật P1-4 như nhánh writer đầu: báo lỗi
                // remove-temp thay vì để lại mồ côi.)
                fs::remove_file(tmp).map_err(|e| StoreError::Io {
                    path: tmp.to_path_buf(),
                    msg: format!("replaced the entry but failed to remove its temp: {e}"),
                })?;
                if let Some(parent) = path.parent() {
                    sync_parent_dir(parent)?;
                }
                Ok(())
            }
            // A sibling won the replacement race — surface a conflict.
            // (Sibling thắng race thay thế — báo conflict.)
            Err(_e) if path.exists() => Err(StoreError::CacheConflict {
                path: path.to_path_buf(),
                expected: "fresh record after stale/corrupt winner removal".to_string(),
                winner: "sibling published first".to_string(),
            }),
            Err(e) => Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("compiled-cache replace retry failed: {e}"),
            }),
        }
    }

    /// Read + parse the record at `path` without verification.
    /// (Đọc + parse record tại `path` chưa verify.)
    fn read_record(&self, path: &Path) -> Result<CompiledRecord, StoreError> {
        let data = fs::read(path)?;
        serde_json::from_slice(&data).map_err(|e| StoreError::CacheCorrupt {
            path: path.to_path_buf(),
            detail: format!("failed to parse winner record: {e}"),
        })
    }

    /// Move a corrupt/poisoned entry into `quarantine/` with a forensic
    /// report (best-effort: a quarantine failure must not mask the
    /// integrity error being reported — serving still fails closed).
    ///
    /// Chuyển entry hỏng/đầu độc vào `quarantine/` kèm báo cáo điều tra
    /// (best-effort: lỗi cách ly không được che lỗi integrity đang báo —
    /// serving vẫn fail cứng).
    fn quarantine_corrupt(
        &self,
        path: &Path,
        key: &CompilationKey,
        reason: &str,
    ) -> Result<(), StoreError> {
        let qdir = self.root.join("quarantine");
        let _ = fs::create_dir_all(&qdir);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let master = key.master_digest();
        let master8 = &master.as_hex()[..8];
        let qfile = qdir.join(format!("compiled-{master8}-{stamp}.json"));
        let report = qdir.join(format!("compiled-{master8}-{stamp}.report"));
        // Best-effort: quarantine failure must not mask the mismatch error.
        // (Best-effort: lỗi quarantine không được che lỗi mismatch gốc.)
        let _ = fs::rename(path, &qfile);
        let _ = fs::write(
            &report,
            format!(
                "compiled-cache integrity failure\nmaster key digest: {master8}\nreason: {reason}\nquarantined at: {stamp}\n"
            ),
        );
        Ok(())
    }
}

/// RAII cleanup for compiled-cache temp files (audit vòng-3 P0-5).
/// (Dọn temp compiled-cache bằng RAII — P0-5 audit vòng-3.)
struct TmpGuard<'a> {
    path: &'a std::path::Path,
    armed: bool,
}

impl<'a> TmpGuard<'a> {
    fn new(path: &'a std::path::Path) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TmpGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            // Best-effort removal of our own unique temp.
            // (Best-effort xóa temp duy nhất của mình.)
            let _ = fs::remove_file(self.path);
        }
    }
}
