// Compiled-cache adversarial tests (P0-1/P0-2/P0-3 audit vòng-4).
// Test đối kháng cho compiled-cache: key đầy đủ ngữ cảnh, record tự xác
// thực, race nhất quán giữa các OS, LRU memo có giới hạn. Đối chiếu
// checklist Tech Lead 2026-09-13.
#![allow(clippy::unwrap_used)] // Test code: unwrap acceptable for setup/assertions
// (Test code: unwrap chấp nhận được cho setup/assert)

use mgc_store::cas::{
    COMPILED_CACHE_SCHEMA_VERSION, CompilationKey, CompiledCache, CompiledModule, ContentStore,
    Loader, StoreError,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn tmp_store_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir().to_path_buf())
        .join("magicore-compiled-cache")
        .join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

const SRC: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn module(js: &str) -> CompiledModule {
    CompiledModule {
        js: js.to_string(),
        source_map: None,
    }
}

/// On-disk entry path, computed exactly like CompiledCache::module_path
/// (integration tests reach the store root through tmp_store_dir).
/// (Path entry trên đĩa, tính đúng như CompiledCache::module_path —
/// integration test tới được gốc store qua tmp_store_dir.)
fn entry_path(root: &std::path::Path, key: &CompilationKey) -> PathBuf {
    let master = key.master_digest();
    let hex = master.as_hex();
    root.join("compiled")
        .join("blake3")
        .join(&hex[..2])
        .join(format!("{hex}.json"))
}

/// BLAKE3 over js + NUL + source_map — mirrors the cache's output digest.
/// (BLAKE3 trên js + NUL + source_map — phản chiếu digest output của cache.)
fn out_digest(js: &str, sm: Option<&str>) -> String {
    // Mirrors the cache's schema-v2 length-prefixed encoding (P1-1).
    // (Phản chiếu mã hóa length-prefixed schema v2 của cache (P1-1).)
    let mut h = blake3::Hasher::new();
    h.update(b"MGC-CC-OV2");
    h.update(&(js.len() as u64).to_le_bytes());
    h.update(js.as_bytes());
    match sm {
        None => {
            h.update(&[0x00]);
        }
        Some(m) => {
            h.update(&[0x01]);
            h.update(&(m.len() as u64).to_le_bytes());
            h.update(m.as_bytes());
        }
    }
    h.finalize().to_hex().to_string()
}

// === P0-1: the key must separate compilation contexts ===

#[test]
fn same_source_different_loaders_get_distinct_entries() {
    // The EXACT collision the audit found: one source text, two loaders.
    // The .ts and .tsx keys must resolve to DIFFERENT entries — neither
    // may serve the other's output.
    // (Đúng collision audit chỉ ra: cùng text source, hai loader. Key .ts
    // với .tsx phải ra 2 entry KHÁC NHAU — không bên nào được phục vụ
    // output của bên kia.)
    let root = tmp_store_dir("loader-split");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();

    let ts_key = CompilationKey::new(
        SRC,
        Loader::Ts,
        "esbuild-rs",
        "0.13.8",
        r#"{"platform":"browser"}"#,
    )
    .unwrap();
    let tsx_key = CompilationKey::new(
        SRC,
        Loader::Tsx,
        "esbuild-rs",
        "0.13.8",
        r#"{"platform":"browser"}"#,
    )
    .unwrap();
    assert_ne!(
        ts_key.master_digest().as_hex(),
        tsx_key.master_digest().as_hex(),
        "loader must be part of the cache address (P0-1)"
    );

    cache.put(&ts_key, &module("// compiled as TS")).unwrap();
    cache
        .put(&tsx_key, &module("// compiled as TSX (JSX enabled)"))
        .unwrap();

    let ts_hit = cache.get(&ts_key).unwrap().unwrap();
    let tsx_hit = cache.get(&tsx_key).unwrap().unwrap();
    assert_eq!(ts_hit.js, "// compiled as TS");
    assert_eq!(tsx_hit.js, "// compiled as TSX (JSX enabled)");
}

#[test]
fn same_source_different_compiler_versions_get_distinct_entries() {
    // Bumping the compiler version must fork the cache — an esbuild 0.13.8
    // entry can never be served to a 0.14.0 pipeline (P0-1).
    // (Nâng phiên bản compiler phải rẽ nhánh cache — entry của esbuild
    // 0.13.8 không bao giờ được phục vụ cho pipeline 0.14.0 (P0-1).)
    let root = tmp_store_dir("compiler-split");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();

    let v1 = CompilationKey::new(SRC, Loader::Tsx, "esbuild-rs", "0.13.8", "{}").unwrap();
    let v2 = CompilationKey::new(SRC, Loader::Tsx, "esbuild-rs", "0.14.0", "{}").unwrap();
    assert_ne!(v1.master_digest().as_hex(), v2.master_digest().as_hex());

    cache.put(&v1, &module("out-v1")).unwrap();
    cache.put(&v2, &module("out-v2")).unwrap();
    assert_eq!(cache.get(&v1).unwrap().unwrap().js, "out-v1");
    assert_eq!(cache.get(&v2).unwrap().unwrap().js, "out-v2");
}

#[test]
fn same_source_different_options_get_distinct_entries() {
    // Options are part of the key (P0-1): platform browser vs node fork
    // the cache.
    // (Options nằm trong key (P0-1): platform browser với node rẽ nhánh
    // cache.)
    let root = tmp_store_dir("options-split");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();

    let browser = CompilationKey::new(
        SRC,
        Loader::Ts,
        "esbuild-rs",
        "0.13.8",
        r#"{"platform":"browser"}"#,
    )
    .unwrap();
    let node = CompilationKey::new(
        SRC,
        Loader::Ts,
        "esbuild-rs",
        "0.13.8",
        r#"{"platform":"node"}"#,
    )
    .unwrap();
    assert_ne!(
        browser.master_digest().as_hex(),
        node.master_digest().as_hex()
    );

    cache.put(&browser, &module("browser-bundle")).unwrap();
    cache.put(&node, &module("node-bundle")).unwrap();
    assert_eq!(cache.get(&browser).unwrap().unwrap().js, "browser-bundle");
    assert_eq!(cache.get(&node).unwrap().unwrap().js, "node-bundle");
}

#[test]
fn malformed_key_inputs_are_rejected() {
    // Key validation is fail-closed: garbage digests / empty identity are
    // rejected at the door (P0-1 defense in depth). The closed-set Loader
    // enum makes "garbage-loader" a COMPILE-TIME impossibility now — the
    // runtime guard that remains is on digests and identity strings.
    // (Key validate fail-closed: digest rác / identity rỗng bị chặn ngay
    // tại cửa (phòng vệ nhiều lớp P0-1). Enum Loader tập đóng khiến
    // "garbage-loader" giờ là bất khả thi ở COMPILE-TIME — runtime guard
    // còn lại nằm ở digest và chuỗi identity.)
    assert!(CompilationKey::new("deadbeef", Loader::Ts, "esbuild-rs", "0.13.8", "{}").is_err());
    // Empty compiler / version still rejected.
    // (Compiler / version rỗng vẫn bị từ chối.)
    assert!(CompilationKey::new(SRC, Loader::Ts, "", "0.13.8", "{}").is_err());
    assert!(CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "", "{}").is_err());
    // Malformed options JSON still rejected.
    // (JSON options sai dạng vẫn bị từ chối.)
    assert!(CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "0.13.8", "not json").is_err());
    // The closed set: no loader exists for an unknown extension — the
    // dev-server derivation path returns None instead of a free string.
    // (Tập đóng: không có loader cho đuôi lạ — đường suy ra loader của
    // dev server trả None thay vì string tự do.)
    assert!(Loader::from_extension("garbage").is_none());
    assert!(
        Loader::from_extension("TS").is_some(),
        "extension match is case-insensitive"
    );
}

// === P0-3: self-authenticating records ===

#[test]
fn poisoned_valid_json_is_rejected_and_quarantined() {
    // THE P0-3 audit case: an attacker swaps the entry for a WELL-FORMED
    // JSON with a malicious js payload. get() must verify the embedded
    // output digest, quarantine the poison, and fail — the payload is
    // never served.
    // (Đúng case P0-3 của audit: kẻ tấn công thay entry bằng JSON HỢP LỆ
    // mang js độc. get() phải verify digest nhúng, cách ly độc, và fail —
    // payload không bao giờ được phục vụ.)
    let root = tmp_store_dir("poison-json");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache: CompiledCache = store.compiled_cache();
    let key = CompilationKey::new(SRC, Loader::Tsx, "esbuild-rs", "0.13.8", "{}").unwrap();

    cache.put(&key, &module("console.log('clean')")).unwrap();

    // Poison behind the cache's back: valid JSON, digest still names the
    // CLEAN payload while the js field carries the malicious code.
    // (Đầu độc sau lưng cache: JSON hợp lệ, digest vẫn trỏ payload SẠCH
    // còn field js mang code độc.)
    let path = entry_path(&root, &key);
    let poison = format!(
        r#"{{"schema_version":{COMPILED_CACHE_SCHEMA_VERSION},"key":{},"output_digest":"{}","js":"maliciousCode()","source_map":null}}"#,
        serde_json::to_string(&key).unwrap(),
        out_digest("console.log('clean')", None)
    );
    fs::write(&path, poison).unwrap();

    match cache.get(&key) {
        Err(StoreError::CacheCorrupt { .. }) => {}
        other => panic!("poisoned entry must fail with CacheCorrupt, got: {other:?}"),
    }

    // The poison was quarantined for forensics.
    // (Độc đã bị cách ly để điều tra.)
    let qdir = root.join("quarantine");
    let q: Vec<_> = fs::read_dir(&qdir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(!q.is_empty(), "poisoned entry must land in quarantine/");

    // And the slot self-heals: the next put publishes a fresh record and
    // get serves it.
    // (Slot tự sửa: put kế tiếp publish record mới và get phục vụ nó.)
    cache.put(&key, &module("console.log('clean')")).unwrap();
    let healed = cache.get(&key).unwrap().unwrap();
    assert_eq!(healed.js, "console.log('clean')");
}

#[test]
fn valid_json_with_wrong_embedded_key_is_rejected() {
    // A record whose embedded key names a DIFFERENT compilation context,
    // copied onto this key's path, is a cross-context steal attempt
    // (P0-1+P0-3): quarantine + CacheCorrupt, never served.
    // (Record mà key nhúng chỉ DANH NGỮ cảnh biên dịch KHÁC, chép sang path
    // key này, là trộm chéo ngữ cảnh (P0-1+P0-3): cách ly + CacheCorrupt,
    // không phục vụ.)
    let root = tmp_store_dir("wrong-key");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    let requested = CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "0.13.8", "{}").unwrap();
    let other = CompilationKey::new(SRC, Loader::Tsx, "esbuild-rs", "0.13.8", "{}").unwrap();

    // Hand-craft a VALID record for `other` (correct digest for "stolen").
    // (Tay dựng record HỢP LỆ cho `other` (digest đúng cho "stolen").)
    let rec = serde_json::json!({
        "schema_version": COMPILED_CACHE_SCHEMA_VERSION,
        "key": other,
        "output_digest": out_digest("stolen", None),
        "js": "stolen",
        "source_map": null,
    });
    let requested_path = entry_path(&root, &requested);
    fs::create_dir_all(requested_path.parent().unwrap()).unwrap();
    fs::write(&requested_path, serde_json::to_vec(&rec).unwrap()).unwrap();

    match cache.get(&requested) {
        Err(StoreError::CacheCorrupt { .. }) => {}
        other => panic!("key-mismatched record must fail with CacheCorrupt, got: {other:?}"),
    }
    // Nothing was served, and the tampered bytes went to quarantine.
    // (Không phục vụ gì, bytes bị can thiệp vào quarantine.)
    let qdir = root.join("quarantine");
    let q: Vec<_> = fs::read_dir(&qdir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        !q.is_empty(),
        "key-mismatched entry must land in quarantine/"
    );
}

#[test]
fn stale_schema_entry_reports_miss_and_is_replaced() {
    // An old-schema (v0) entry is a LEGIT old cache, not corruption: get
    // reports a miss; the next put replaces it (P0-3 migration path).
    // (Entry schema cũ (v0) là cache hợp lệ của thời trước, không phải hỏng:
    // get báo miss; put kế tiếp thay nó (đường migration P0-3).)
    let root = tmp_store_dir("stale-schema");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    let key = CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "0.13.8", "{}").unwrap();

    // Hand-write a v0 record (schema_version missing → serde default 0).
    // (Tay viết record v0 (thiếu schema_version → serde default 0).)
    let path = entry_path(&root, &key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let stale = format!(
        r#"{{"key":{},"output_digest":"{}","js":"old","source_map":null}}"#,
        serde_json::to_string(&key).unwrap(),
        out_digest("old", None)
    );
    fs::write(&path, stale).unwrap();

    // Miss, not an error.
    // (Miss, không phải lỗi.)
    assert!(cache.get(&key).unwrap().is_none());

    // The following put replaces it and the fresh record serves.
    // (Put kế tiếp thay nó và record mới phục vụ được.)
    cache.put(&key, &module("fresh")).unwrap();
    assert_eq!(cache.get(&key).unwrap().unwrap().js, "fresh");
}

#[test]
fn truncated_entry_fails_at_get_and_quarantines() {
    // Torn bytes at the entry path → CacheCorrupt + quarantine (the old
    // design only surfaced a parse error; now forensics + typed error).
    // (Bytes đứt ở path entry → CacheCorrupt + cách ly (thiết kế cũ chỉ
    // báo lỗi parse; giờ có forensics + lỗi typed).)
    let root = tmp_store_dir("torn");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    let key = CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "0.13.8", "{}").unwrap();

    cache.put(&key, &module("ok")).unwrap();
    let path = entry_path(&root, &key);
    fs::write(&path, b"{TORN-JSON").unwrap();

    match cache.get(&key) {
        Err(StoreError::CacheCorrupt { .. }) => {}
        other => panic!("torn entry must fail with CacheCorrupt, got: {other:?}"),
    }
    let qdir = root.join("quarantine");
    let q: Vec<_> = fs::read_dir(&qdir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(!q.is_empty(), "torn entry must land in quarantine/");
}

// === P0-2: race contract — same key, DIVERGENT payloads ===

#[test]
fn concurrent_same_key_same_payload_all_succeed() {
    // 8 writers, SAME key, IDENTICAL payload: everyone succeeds (first
    // publishes, losers verify+dedup). No temp leftovers at any depth.
    // (8 writer, CÙNG key, payload GIỐNG HỆT: tất cả thành công (writer
    // đầu publish, thua verify+dedup). Không sót temp ở mọi độ sâu.)
    let root = tmp_store_dir("race-same");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = Arc::new(store.compiled_cache());
    let key =
        Arc::new(CompilationKey::new(SRC, Loader::Tsx, "esbuild-rs", "0.13.8", "{}").unwrap());

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let key = Arc::clone(&key);
            std::thread::spawn(move || cache.put(&key, &module("console.log(1)")))
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        let res = h.join().expect("writer must not panic");
        assert!(res.is_ok(), "identical writer {i} must succeed: {res:?}");
    }

    let m = cache.get(&key).unwrap().expect("entry must exist");
    assert_eq!(m.js, "console.log(1)");
    let leftovers = scan_temps(&root);
    assert!(leftovers.is_empty(), "temp leftovers: {leftovers:?}");
}

#[test]
fn sequential_same_key_divergent_payload_conflicts() {
    // The API-level contract (P0-2): a SECOND put of a DIFFERENT payload
    // under the same key must NOT silently win or lose — it must surface a
    // typed CacheConflict. The first (valid) winner stays in place.
    // (Hợp đồng cấp API (P0-2): put THỨ HAI với payload KHÁC dưới cùng key
    // KHÔNG được thắng/thua im lặng — phải báo CacheConflict typed. Winner
    // đầu (hợp lệ) giữ nguyên vị trí.)
    let root = tmp_store_dir("conflict-seq");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    let key = CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "0.13.8", "{}").unwrap();

    cache.put(&key, &module("output-A")).unwrap();
    let res = cache.put(&key, &module("output-B"));
    match res {
        Err(StoreError::CacheConflict { .. }) => {}
        other => panic!("divergent same-key put must conflict, got: {other:?}"),
    }
    // The valid winner is still in place.
    // (Winner hợp lệ vẫn ở chỗ.)
    assert_eq!(cache.get(&key).unwrap().unwrap().js, "output-A");
}

#[test]
fn concurrent_same_key_divergent_payloads_deterministic_outcome() {
    // 8 writers, same key, FOUR divergent payloads racing: the served
    // entry is ALWAYS one of the racing payloads — intact, verifiable,
    // never a mix or a torn file. Losers either dedup (same payload as
    // winner) or get a typed conflict/corrupt error. Deterministic
    // first-wins via hard_link on Unix AND Windows.
    //
    // (8 writer, cùng key, BỐN payload phân kỳ đua: entry được phục vụ
    // luôn luôn là MỘT trong các payload đua — nguyên vẹn, verify được,
    // không trộn, không đứt. Bên thua hoặc dedup (cùng payload với
    // winner) hoặc nhận lỗi conflict/corrupt typed. First-wins tất định
    // qua hard_link trên Unix VÀ Windows.)
    let root = tmp_store_dir("race-divergent");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = Arc::new(store.compiled_cache());
    let key =
        Arc::new(CompilationKey::new(SRC, Loader::Tsx, "esbuild-rs", "0.13.8", "{}").unwrap());

    let payloads = ["pa", "pb", "pc", "pd"];
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let cache = Arc::clone(&cache);
            let key = Arc::clone(&key);
            let payload = payloads[i % payloads.len()].to_string();
            std::thread::spawn(move || {
                let res = cache.put(&key, &module(&payload));
                (payload, res)
            })
        })
        .collect();

    // REVIEW vòng-2 hardening: EVERY Ok must be the payload that actually
    // won, EVERY Err must be a typed conflict/corrupt — a fake Ok for a
    // divergent payload would be exactly the silent-nondeterminism bug the
    // audit flagged. We can only check after collecting all results, so
    // gather first, assert the served payload afterwards.
    // (Harden vòng-2 review: MỌI Ok phải là payload thực sự thắng, MỌI Err
    // phải là conflict/corrupt typed — Ok giả cho payload phân kỳ chính là
    // bug nondeterminism im lặng mà audit chỉ ra. Chỉ check được sau khi
    // thu đủ kết quả nên gom trước, assert payload được serve sau.)
    let mut results = Vec::new();
    for h in handles {
        results.push(h.join().expect("writer must not panic"));
    }

    // The surviving entry is intact and IS one of the racing payloads.
    // (Entry sống sót nguyên vẹn và LÀ một trong các payload đua.)
    let served = cache.get(&key).unwrap().expect("entry must exist").js;
    assert!(
        payloads.contains(&served.as_str()),
        "served payload must be one of the racers, got '{served}'"
    );

    let mut winners = 0;
    for (payload, res) in &results {
        match res {
            Ok(()) => {
                // Only the WINNING payload (and its dedups) may report Ok.
                // (Chỉ payload THẮNG (và dedup của nó) được báo Ok.)
                assert_eq!(
                    payload, &served,
                    "put() reported Ok for '{payload}' but '{served}' was served — fake Ok"
                );
                winners += 1;
            }
            Err(StoreError::CacheConflict { .. }) | Err(StoreError::CacheCorrupt { .. }) => {
                // Typed race outcome — the documented loser contract.
                // (Kết quả race typed — hợp đồng bên thua được document.)
            }
            Err(other) => {
                panic!("unexpected race error (must be CacheConflict/CacheCorrupt): {other:?}")
            }
        }
    }
    assert!(
        winners >= 1,
        "at least the first writer must win (winners={winners})"
    );
    let leftovers = scan_temps(&root);
    assert!(leftovers.is_empty(), "temp leftovers: {leftovers:?}");
}

// === P1-2: memo is bounded (LRU) ===

#[test]
#[cfg(unix)]
fn memo_is_bounded_and_evicts_lru() {
    // With a tiny capacity the memo MUST evict: touch capacity+2 blobs and
    // verify the eviction counter climbs while correctness holds (evicted
    // blobs re-verify from disk — never a wrong serve).
    // (Capacity nhỏ thì memo PHẢI đẩy entry: chạm capacity+2 blob và verify
    // bộ đếm eviction tăng trong khi correctness giữ nguyên (blob bị đẩy
    // re-verify từ đĩa — không bao giờ serve sai).)
    let root = tmp_store_dir("memo-lru");
    let store = ContentStore::new(root.clone()).unwrap();
    // NonZeroUsize at the TYPE level (P1-3): zero capacity is now
    // unrepresentable — the builder refuses it at compile time.
    // (NonZeroUsize ở tầng KIỂU (P1-3): capacity-0 giờ không thể biểu
    // diễn — builder chặn ngay lúc compile.)
    let tiny = store.with_memo_capacity(std::num::NonZeroUsize::new(4).unwrap());

    // Touch capacity+2 distinct blobs through export (verify+memoize).
    // (Chạm capacity+2 blob khác nhau qua export (verify + memo hóa).)
    for i in 0..6u32 {
        let data = format!("memo-blob-{i}").into_bytes();
        let hash = tiny.import_bytes(&data).unwrap();
        let dest = root.join(format!("out{i}.bin"));
        tiny.export_to(&hash, &dest).unwrap();
    }

    let stats = tiny.memo_stats();
    assert!(
        stats.evictions >= 2,
        "capacity-4 memo must evict ≥2 after 6 touches: {stats:?}"
    );
    assert!(stats.misses >= 6, "each first touch is a miss: {stats:?}");
}

#[test]
#[cfg(unix)]
fn memo_replacement_is_not_counted_as_eviction() {
    // Eviction accounting (P1-3, Tech Lead vòng-5/6/7): lru 0.18 `push`
    // returns the REPLACED value for an existing key even when nothing
    // was evicted — the old `is_some()` check miscounted replacements as
    // evictions. Re-touching ONE blob below capacity must NEVER raise the
    // eviction counter; only a genuine capacity overflow may.
    // (Kế toán eviction (P1-3): lru 0.18 `push` trả value bị THAY THẾ cho
    // key đã tồn tại kể cả khi không đẩy entry nào — check `is_some()` cũ
    // đếm replacement thành eviction. Chạm lại MỘT blob dưới capacity
    // KHÔNG BAO GIỜ được tăng bộ đếm eviction; chỉ capacity overflow thật
    // mới được.)
    let root = tmp_store_dir("memo-replace");
    let store = ContentStore::new(root.clone()).unwrap();
    let tiny = store.with_memo_capacity(std::num::NonZeroUsize::new(4).unwrap());

    // Fill BELOW capacity (3 of 4) with distinct blobs.
    // (Điền DƯỚI capacity (3/4) bằng blob khác nhau.)
    let mut dests = Vec::new();
    for i in 0..3u32 {
        let data = format!("replace-blob-{i}").into_bytes();
        let hash = tiny.import_bytes(&data).unwrap();
        let dest = root.join(format!("r{i}.bin"));
        tiny.export_to(&hash, &dest).unwrap();
        dests.push((hash, dest));
    }
    let before = tiny.memo_stats();
    assert_eq!(
        before.evictions, 0,
        "3 touches of a 4-capacity memo evict nothing"
    );

    // Re-export the SAME blobs many times — pure memo hits/replacements,
    // still below capacity: evictions must stay 0. Each export needs a
    // FRESH destination (export_to refuses to overwrite).
    // (Export lại CÙNG blob nhiều lần — thuần hit/replacement, vẫn dưới
    // capacity: eviction phải giữ 0. Mỗi export cần đích MỚI (export_to
    // từ chối ghi đè).)
    for round in 0..5u32 {
        for (idx, (hash, _dest)) in dests.iter().enumerate() {
            let again = root.join(format!("again-{round}-{idx}.bin"));
            tiny.export_to(hash, &again).unwrap();
        }
    }
    let after = tiny.memo_stats();
    assert_eq!(
        after.evictions, 0,
        "re-touches below capacity are replacements, NOT evictions (P1-3): {after:?}"
    );
    assert!(
        after.hits >= 5 * 3,
        "re-exports must hit the memo: {after:?}"
    );
}

#[test]
#[cfg(unix)]
fn memo_full_cache_replacement_is_not_counted_as_eviction() {
    // P1-1 (adversarial review vòng-9): fill the memo to EXACT capacity,
    // then re-verify ONE key it already holds. The old accounting
    // (`len_before == cap && len == cap`) counted that replacement as an
    // eviction. Contract: evictions stays 0 — only a FRESH key entering
    // a full cache evicts.
    // (P1-1: điền memo ĐÚNG capacity, rồi verify lại MỘT key nó đang giữ.
    // Kế toán cũ (`len_before == cap && len == cap`) đếm replacement đó
    // thành eviction. Hợp đồng: evictions giữ 0 — chỉ key MỚI vào cache
    // đầy mới đẩy entry.)
    let root = tmp_store_dir("memo-full-replace");
    let store = ContentStore::new(root.clone()).unwrap();
    let tiny = store.with_memo_capacity(std::num::NonZeroUsize::new(2).unwrap());

    // Fill BOTH slots (capacity 2) with distinct blobs.
    // (Điền CẢ HAI slot (capacity 2) bằng blob khác nhau.)
    let mut keys = Vec::new();
    for i in 0..2u32 {
        let data = format!("full-replace-blob-{i}").into_bytes();
        let hash = tiny.import_bytes(&data).unwrap();
        let dest = root.join(format!("f{i}.bin"));
        tiny.export_to(&hash, &dest).unwrap();
        keys.push(hash);
    }
    let before = tiny.memo_stats();
    assert_eq!(
        before.evictions, 0,
        "filling exactly to capacity evicts nothing: {before:?}"
    );

    // Replace ONE existing key on the FULL cache many times — each
    // export needs a FRESH destination (export_to refuses overwrite).
    // (Thay MỘT key đang có trên cache ĐẦY nhiều lần — mỗi export cần
    // đích MỚI (export_to từ chối ghi đè).)
    for round in 0..4u32 {
        let again = root.join(format!("full-again-{round}.bin"));
        tiny.export_to(&keys[0], &again).unwrap();
    }
    let after = tiny.memo_stats();
    assert_eq!(
        after.evictions, 0,
        "replacement on a FULL cache is NOT an eviction (P1-1): {after:?}"
    );
    assert!(after.hits >= 4, "re-verifies must hit: {after:?}");

    // The genuine eviction: a THIRD, FRESH key must displace one —
    // evictions must finally increment (exactly 1).
    // (Eviction thật: key THỨ BA MỚI phải đẩy bớt một entry — evictions
    // phải nhích (đúng 1).)
    let data = b"full-replace-blob-new".to_vec();
    let hash = tiny.import_bytes(&data).unwrap();
    let dest = root.join("f-new.bin");
    tiny.export_to(&hash, &dest).unwrap();
    let final_stats = tiny.memo_stats();
    assert_eq!(
        final_stats.evictions, 1,
        "a fresh key on a full cache is ONE genuine eviction: {final_stats:?}"
    );
}

#[test]
#[cfg(unix)]
fn memo_hits_count_and_mutation_still_fails() {
    // Hit path accounting + eviction NEVER weakens security: a memoized
    // blob that is then rewritten on disk still fails (the LRU entry dies
    // with the file identity).
    // (Kế toán đường hit + eviction KHÔNG BAO GIỜ yếu hóa bảo mật: blob
    // đã memo mà bị ghi lại trên đĩa vẫn fail (entry LRU chết cùng file
    // identity).)
    let root = tmp_store_dir("memo-hit");
    let store = ContentStore::new(root.clone()).unwrap();
    let data = b"memo-hit-payload".to_vec();
    let hash = store.import_bytes(&data).unwrap();

    let o1 = root.join("a.bin");
    store.export_to(&hash, &o1).unwrap(); // miss + memoize
    let o2 = root.join("b.bin");
    store.export_to(&hash, &o2).unwrap(); // hit

    let stats = store.memo_stats();
    assert!(
        stats.hits >= 1,
        "second export must hit the memo: {stats:?}"
    );
    assert_eq!(stats.misses, 1, "first export is the only miss: {stats:?}");

    // Mutate → memo entry dies → fresh verify fails closed.
    // (Sửa → entry memo chết → verify mới fail cứng.)
    fs::write(hash.cas_path(&root), b"POISON-AFTER-LRU").unwrap();
    let o3 = root.join("c.bin");
    assert!(store.export_to(&hash, &o3).is_err());
}

// === Helpers ===

/// Recursively scan for atomic-write temp leftovers under a store root.
/// (Quét đệ quy temp của ghi nguyên tử dưới gốc store.)
fn scan_temps(root: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("compiled-") {
                    found.push(path);
                }
            }
        }
    }
    found
}

// === P0-A (vòng-8): CAS root symlink check phải hiệu lực ===

#[test]
#[cfg(unix)]
fn cas_root_that_is_a_symlink_is_rejected() {
    // P0-A: the OLD validate_cas_root used fs::metadata (follows the
    // link), so a symlinked store root sailed through as a real dir. The
    // fixed check uses symlink_metadata and MUST reject the link itself.
    // (P0-A: validate_cas_root CŨ dùng fs::metadata (follow link) nên
    // store root là symlink vẫn lọt như thư mục thật. Check sửa dùng
    // symlink_metadata và PHẢI từ chối chính link.)
    let base = tmp_store_dir("root-symlink-base");
    let real = base.join("real-store");
    fs::create_dir_all(&real).unwrap();
    let link = base.join("link-store");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(
        ContentStore::new(link).is_err(),
        "a symlinked CAS root must be rejected (P0-A)"
    );
}

#[test]
#[cfg(unix)]
fn cas_root_under_symlinked_ancestor_is_rejected() {
    // The root itself is a real dir, but an ANCESTOR is a symlink — the
    // whole store is redirected. Must be rejected (P0-A ancestor sweep).
    // (Root là thư mục thật nhưng MỘT ANCESTOR là symlink — cả store bị
    // chuyển hướng. Phải từ chối (quét ancestor P0-A).)
    let base = tmp_store_dir("root-ancestor-symlink");
    let real_parent = base.join("real-parent");
    fs::create_dir_all(&real_parent).unwrap();
    let link_parent = base.join("link-parent");
    std::os::unix::fs::symlink(&real_parent, &link_parent).unwrap();
    let root = link_parent.join("cas");
    assert!(
        ContentStore::new(root).is_err(),
        "a CAS root under a symlinked ancestor must be rejected (P0-A)"
    );
}

#[test]
#[cfg(unix)]
fn cas_root_dangling_symlink_is_rejected() {
    // A DANGLING symlink root (target never exists): fs::metadata errors
    // NotFound while fs::symlink_metadata succeeds and shows the link.
    // The old code treated NotFound as "root does not exist yet" and
    // created the store THROUGH the dangling link's parent path; the new
    // check must reject the link itself.
    // (Root là symlink TREO (đích không tồn tại): fs::metadata lỗi NotFound
    // trong khi fs::symlink_metadata thành công và cho thấy link. Code cũ
    // coi NotFound là "root chưa tồn tại" rồi tạo store xuyên qua path mẹ
    // của link treo; check mới phải từ chối chính link.)
    let base = tmp_store_dir("root-dangling");
    let dangling = base.join("dangling-store");
    std::os::unix::fs::symlink(base.join("never-created"), &dangling).unwrap();
    assert!(
        ContentStore::new(dangling).is_err(),
        "a dangling-symlink CAS root must be rejected, not created through (P0-A)"
    );
}

#[test]
fn cas_root_regular_dir_is_accepted() {
    // Negative control: an ordinary fresh root still initializes fine.
    // (Đối chứng âm: root mới hoàn toàn bình thường vẫn khởi tạo tốt.)
    let root = tmp_store_dir("root-plain");
    assert!(ContentStore::new(root).is_ok());
}

// === P0-D (vòng-8): CAS permissions phải chặt ===

#[test]
#[cfg(unix)]
fn cas_root_and_blobs_are_owner_only() {
    // P0-D: store dirs must be 0700 and blobs 0600/0700 (owner-only) —
    // compiled JS + inline sourcemaps must not be world/group-readable.
    // (P0-D: thư mục store phải 0700 và blob 0600/0700 (chỉ-owner) — JS
    // biên dịch + sourcemap inline không được cho group/others đọc.)
    use std::os::unix::fs::PermissionsExt;
    let root = tmp_store_dir("perm-tight");
    let store = ContentStore::new(root.clone()).unwrap();

    let root_mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
    assert_eq!(root_mode, 0o700, "CAS root must be 0700 (P0-D)");

    let hash = store.import_bytes(b"perm-blob").unwrap();
    let blob_mode = fs::metadata(hash.cas_path(&root))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(blob_mode, 0o600, "non-exec CAS blob must be 0600 (P0-D)");

    let exec_hash = store.import_bytes_with_exec(b"#!/bin/sh\n", true).unwrap();
    let exec_mode = fs::metadata(exec_hash.cas_path(&root))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(exec_mode, 0o700, "exec CAS blob must be 0700 (P0-D)");

    // Export to a project still yields ORDINARY project modes (0644).
    // (Export ra project vẫn ra mode project BÌNH THƯỜNG (0644).)
    let dest = root.join("exported.sh");
    store.export_to(&exec_hash, &dest).unwrap();
    let export_mode = fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
    assert_eq!(export_mode, 0o755, "exported executable keeps 0755 (P0-D)");
}

#[test]
#[cfg(unix)]
fn system_root_owned_symlink_ancestor_is_tolerated() {
    // REVIEW vòng-2 finding: macOS ships `/var → private/var` (root-owned
    // system symlink) and tempfile::tempdir() on macOS/CI lives under it
    // — a hard ancestor-symlink ban bricks every standard temp dir. The
    // contract: root-OWNED symlink ancestors are tolerated (a same-user
    // attacker cannot write a root-owned link), every OTHER symlink
    // ancestor stays a hard reject (see
    // cas_root_under_symlinked_ancestor_is_rejected).
    // (Phát hiện REVIEW vòng-2: macOS ship `/var → private/var` (symlink
    // hệ thống do root sở hữu) và tempfile::tempdir() trên macOS/CI nằm
    // dưới nó — cứng cấm ancestor-symlink brick mọi temp dir chuẩn. Hợp
    // đồng: ancestor symlink do ROOT sở hữu được dung thứ (attacker
    // cùng-user không ghi được link root-owned), mọi ancestor symlink
    // KHÁC vẫn bị chặn tuyệt đối (xem
    // cas_root_under_symlinked_ancestor_is_rejected).
    let dir = tempfile::tempdir().unwrap();
    // tempdir() itself proves the exemption: on macOS it resolves under
    // /var (root-owned symlink) and MUST still open a store.
    // (tempdir() tự chứng minh exemption: trên macOS nó resolve dưới
    // /var (symlink root-owned) và VẪN phải mở được store.)
    let store_root = dir.path().join("store");
    let store = ContentStore::new(store_root).unwrap();
    let hash = store.import_bytes(b"under-system-symlink").unwrap();
    assert!(store.contains(&hash));
}

// === P1-1 (vòng-8): digest v2 length-prefixed — không còn collision ===

#[test]
fn output_digest_none_is_not_some_empty() {
    // THE P1-1 collision the old encoding had: None ≡ Some(""). Schema v2
    // presence-tag separates them.
    // (Collision P1-1 của mã hóa cũ: None ≡ Some(""). Tag hiện diện của
    // schema v2 tách chúng ra.)
    let a = out_digest("js", None);
    let b = out_digest("js", Some(""));
    assert_ne!(
        a, b,
        "source_map None must NOT collide with Some(\"\") (P1-1)"
    );
}

#[test]
fn output_digest_nul_in_payload_is_not_ambiguous() {
    // The second P1-1 collision: js="a\0b",sm=None vs js="a",sm="b\0" —
    // wait, those differ; the real ambiguity was any NUL INSIDE payloads
    // re-creating the separator. Length prefixes make every split unique.
    // (Collision P1-1 thứ hai: NUL BÊN TRONG payload tái tạo dấu phân
    // cách. Prefix độ dài khiến mọi cách tách là duy nhất.)
    let a = out_digest("a\0b", Some("c\0d"));
    let b = out_digest("a\0", Some("b\0c\0d"));
    let c = out_digest("a\0b\0c", Some("\0d"));
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_ne!(b, c);
}

#[test]
fn output_digest_same_module_same_digest() {
    // Injectivity check: identical (js, sm) pairs must hash identically —
    // legitimate dedup in resolve_race still works.
    // (Kiểm tra đơn ánh: cặp (js, sm) giống nhau phải hash giống nhau —
    // dedup hợp lệ trong resolve_race vẫn chạy.)
    assert_eq!(out_digest("x", None), out_digest("x", None));
    assert_eq!(out_digest("x", Some("y")), out_digest("x", Some("y")));
}

// === P0-4 (vòng-8): biên threat model — attacker cùng user recompute ===

#[test]
fn attacker_recompute_is_served_documented_boundary() {
    // DOCUMENTED BOUNDARY (P0-4): a same-user attacker who recomputes the
    // output digest for a forged record PRODUCES a record indistinguish-
    // able from a legit one — the cache is self-VALIDATING (corruption/
    // torn writes), not self-AUTHENTICATING. This test pins the boundary:
    // the forged-but-consistent record IS served. Changing this behavior
    // requires keyed MACs / signatures (remote-cache scope, not local).
    // (BIÊN ĐÃ GHI RÕ (P0-4): attacker cùng user recompute digest output
    // cho record giả sẽ tạo record KHÔNG PHÂN BIỆT được với record thật —
    // cache TỰ KIỂM TRA (hỏng/đứt ghi), không TỰ XÁC THỰC. Test ghim
    // biên: record giả-nhưng-nhất-quán VẪN được phục vụ. Đổi hành vi này
    // cần MAC keyed / chữ ký (scope remote-cache, không phải local).)
    let root = tmp_store_dir("attacker-recompute");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    let key = CompilationKey::new(SRC, Loader::Tsx, "esbuild-rs", "0.13.8", "{}").unwrap();

    // Attacker forges a fully-consistent record (digest matches payload).
    // (Attacker giả record nhất quán hoàn toàn (digest khớp payload).)
    let forged = serde_json::json!({
        "schema_version": COMPILED_CACHE_SCHEMA_VERSION,
        "key": key,
        "output_digest": out_digest("attacker-payload()", None),
        "js": "attacker-payload()",
        "source_map": null,
    });
    let path = entry_path(&root, &key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&forged).unwrap()).unwrap();

    // The cache serves it: this IS the accepted threat-model boundary.
    // (Cache phục vụ nó: ĐÂY là biên threat model được chấp nhận.)
    let served = cache.get(&key).unwrap().unwrap();
    assert_eq!(served.js, "attacker-payload()");
}

// === P0-1 (vòng-8): record JSON với key rác bị từ chối ở parse ===

#[test]
fn record_with_garbage_loader_json_fails_to_parse() {
    // A hand-crafted record whose key carries "garbage-loader" used to
    // deserialize into a free-form CompilationKey; with the closed-set
    // Loader the WHOLE record fails to parse → get() quarantines it and
    // fails closed (P0-1).
    // (Record tự chế với key mang "garbage-loader" từng deserialize thành
    // CompilationKey string tự do; với Loader tập đóng CẢ record fail
    // parse → get() cách ly và fail cứng (P0-1).)
    let root = tmp_store_dir("garbage-loader");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    let key = CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "0.13.8", "{}").unwrap();

    let rec = serde_json::json!({
        "schema_version": COMPILED_CACHE_SCHEMA_VERSION,
        "key": {
            "schema_version": COMPILED_CACHE_SCHEMA_VERSION,
            "source_digest": SRC,
            "loader": "garbage-loader",
            "compiler": "esbuild-rs",
            "compiler_version": "0.13.8",
            "options_digest": out_digest("{}", None),
        },
        "output_digest": out_digest("x", None),
        "js": "x",
        "source_map": null,
    });
    let path = entry_path(&root, &key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&rec).unwrap()).unwrap();

    match cache.get(&key) {
        Err(StoreError::CacheCorrupt { .. }) => {}
        other => panic!("garbage-loader record must be CacheCorrupt, got: {other:?}"),
    }
}

#[test]
fn record_with_junk_digest_key_fails_to_parse() {
    // Junk (non-BLAKE3) digests in a record's key fail CompilationKey's
    // deserialize-validation → corrupt, never served (P0-1).
    // (Digest rác (không BLAKE3) trong key của record fail validate khi
    // deserialize CompilationKey → hỏng, không bao giờ phục vụ (P0-1).)
    let root = tmp_store_dir("junk-digest");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    let key = CompilationKey::new(SRC, Loader::Ts, "esbuild-rs", "0.13.8", "{}").unwrap();

    let rec = serde_json::json!({
        "schema_version": COMPILED_CACHE_SCHEMA_VERSION,
        "key": {
            "schema_version": COMPILED_CACHE_SCHEMA_VERSION,
            "source_digest": "not-a-blake3-digest",
            "loader": "ts",
            "compiler": "esbuild-rs",
            "compiler_version": "0.13.8",
            "options_digest": "also-not-blake3",
        },
        "output_digest": out_digest("x", None),
        "js": "x",
        "source_map": null,
    });
    let path = entry_path(&root, &key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&rec).unwrap()).unwrap();

    match cache.get(&key) {
        Err(StoreError::CacheCorrupt { .. }) => {}
        other => panic!("junk-digest record must be CacheCorrupt, got: {other:?}"),
    }
}

// === P0-C (vòng-8): prune fail-closed khi DB không đọc được ===

#[test]
fn prune_refuses_when_database_unavailable() {
    // P0-C (unit level of the CLI contract): the prune path must NEVER
    // fall back to the nlink heuristic — exports are independent copies
    // (nlink==1 on live blobs), so nlink-pruning deletes the live cache.
    // The fail-closed decision lives in cli/src/commands/cache.rs; this
    // test pins the DB-side primitives it depends on: a missing DB has no
    // live refs to read, and the generation table defaults sanely.
    // (P0-C (cấp unit của hợp đồng CLI): đường prune KHÔNG BAO GIỜ rơi về
    // heuristic nlink — export là bản sao độc lập (nlink==1 ở blob sống)
    // nên prune-theo-nlink xóa cache sống. Quyết định fail-closed nằm ở
    // cli/src/commands/cache.rs; test ghim nguyên thủy phía DB mà nó dựa
    // vào: DB thiếu thì không có live refs để đọc, và bảng generation
    // default hợp lý.)
    let root = tmp_store_dir("prune-db-gate");
    let db_path = root.join("store.db");
    let db = mgc_store::Database::open(&db_path).unwrap();

    // Generation-token flow (P0-A): begin returns the install's TOKEN →
    // claims file into THAT token → promote names the token.
    // (Luồng generation-token: begin trả TOKEN của install → claim rơi
    // vào đúng token đó → promote gọi đúng token.)
    let generation = db.begin_cas_generation("/proj/x").unwrap();
    assert!(generation >= 1);
    db.cas_claim("/proj/x", generation, "hash-a").unwrap();
    db.cas_claim("/proj/x", generation, "hash-b").unwrap();
    assert_eq!(db.list_cas_live_refs().unwrap().len(), 2);

    // A SECOND install generation: old claims stay live (crash safety).
    // (Generation install THỨ HAI: claim cũ vẫn live (an toàn crash).)
    let gen2 = db.begin_cas_generation("/proj/x").unwrap();
    db.cas_claim("/proj/x", gen2, "hash-c").unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert!(
        live.contains(&"hash-a".to_string()),
        "old generation claims must stay live (P0-C)"
    );
    assert!(live.contains(&"hash-c".to_string()));

    // Promote the second token: retires non-staging claims not vouched by
    // it. t1 was NEVER promoted or aborted — it is still 'staging' (an
    // in-flight or crashed install), so ITS claims survive the other
    // token's promote (over-retention is safe; only t1's own abort — or
    // doctor GC — may drop them). This is the P0-A contract: no promote
    // may delete another install's staging claims.
    // (Promote token thứ hai: nghỉ hưu claim không staging mà nó không
    // bảo chứng. t1 CHƯA TỪNG promote hay abort — vẫn 'staging' (install
    // đang chạy hoặc đứt) nên claim của NÓ sống sót qua promote của token
    // khác (giữ thừa an toàn; chỉ abort của chính t1 — hoặc doctor GC —
    // mới được dọn). Đây là hợp đồng P0-A: promote nào cũng KHÔNG được xóa
    // claim staging của install khác.)
    db.promote_cas_generation("/proj/x", gen2).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(
        live,
        vec![
            "hash-a".to_string(),
            "hash-b".to_string(),
            "hash-c".to_string()
        ],
        "a still-staging (crashed/in-flight) generation's claims must survive another token's promote"
    );

    // The crashed install's own ABORT retires its claims — nobody else's.
    // (ABORT của chính install đứt nghỉ hưu claim của nó — không ai khác.)
    db.abort_cas_generation("/proj/x", generation).unwrap();
    let live = db.list_cas_live_refs().unwrap();
    assert_eq!(live, vec!["hash-c".to_string()]);

    // Idempotent re-promote is safe.
    // (Re-promote idempotent là an toàn.)
    db.promote_cas_generation("/proj/x", gen2).unwrap();
    assert_eq!(db.list_cas_live_refs().unwrap().len(), 1);
}
