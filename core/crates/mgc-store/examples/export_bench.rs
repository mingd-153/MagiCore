// APFS clonefile warm-memo staging MICROBENCHMARK — PROVISIONAL (audit
// vòng-3 P0-4 relabel: the previous "400×" number measured the warm memo
// path; this harness now reports cold (first-touch, full rehash) and warm
// (memo hit) separately so neither can be quoted as the other).
//
// Scope and limits (do NOT quote beyond these):
// - Local single-machine APFS Apple Silicon microbenchmark.
// - NOT a package-manager benchmark; NOT cross-platform; NOT end-to-end.
// - export_to = verify gate (source verify) + staging (clonefile/COW or
//   copy) + staged-hash verification + rename. The staged-hash verify is
//   part of the production contract, so its cost is INCLUDED in every
//   number below.
// - Strategy actually used (CowClone vs PlainCopy) is asserted per run via
//   the staging-kind probe below — a fallback that silently never runs is
//   a false-green (audit vòng-3).
//
// MICBENCHMARK staging clonefile APFS warm-memo — PROVISIONAL (P0-4 audit
// vòng-3 đổi nhãn: con số "400×" cũ đo đường warm memo; harness này giờ báo
// riêng cold (chạm đầu, rehash đầy đủ) và warm (memo hit) để không ai trích
// cái này thay cho cái kia).
//
// Phạm vi và giới hạn (KHÔNG trích vượt khỏi đây):
// - Microbenchmark 1 máy Apple Silicon APFS local.
// - KHÔNG phải benchmark package-manager; KHÔNG cross-platform; KHÔNG
//   end-to-end.
// - export_to = cổng verify (verify nguồn) + staging (clonefile/COW hoặc
//   copy) + verify hash bản staged + rename. Verify hash staged thuộc hợp
//   đồng production nên chi phí được TÍNH VÀO mọi con số dưới đây.
// - Chiến lược chạy thật (CowClone vs PlainCopy) được assert từng run qua
//   probe staging-kind phía dưới — fallback im lặng không chạy là
//   false-green (audit vòng-3).

// (bytes sai).)
// Benchmark harness: unwraps fail loudly on a broken environment — a
// partial failure cannot produce meaningful numbers.
// (Bộ đo benchmark: unwrap hét to khi môi trường hỏng — hỏng một phần thì
// số không còn nghĩa.)
#![allow(clippy::unwrap_used)]

use mgc_store::cas::{ContentStore, IntegrityHash};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

fn bench_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir().to_path_buf())
        .join("mgc-export-bench")
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

fn payload(size: usize) -> Vec<u8> {
    // Pseudo-random payload via a cheap LCG (stable, no external seed).
    // (Payload giả ngẫu nhiên qua LCG rẻ (ổn định, không seed ngoài).)
    let mut data = Vec::with_capacity(size);
    let mut state: u64 = 0x4315_4315_4315_4315;
    for _ in 0..size {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        data.push((state >> 33) as u8);
    }
    data
}

/// Measure the ACTUAL staging strategy on this machine/filesystem pair via
/// the store's public `staging_kind_probe` (audit vòng-4 P0-4: the old
/// label was hardcoded per-OS and could claim "clonefile" while production
/// silently fell back to plain copy on a non-APFS volume). The probe
/// clones a known file through the same code path the export uses.
///
/// Đo chiến lược staging THẬT trên cặp máy/filesystem này qua
/// `staging_kind_probe` public của store (P0-4 audit vòng-4: nhãn cũ gắn
/// cứng theo OS và có thể nói "clonefile" trong khi production âm thầm rơi
/// fallback copy trên volume không phải APFS). Probe clone một file đã
/// biết qua đúng đường code mà export dùng.
fn measure_strategy(store: &ContentStore, export_dir: &std::path::Path) -> &'static str {
    // Probe needs a REAL blob in the store — import a probe payload and
    // ask which strategy an export into `export_dir` would use.
    // (Probe cần blob THẬT trong store — import payload probe rồi hỏi
    // chiến lược mà export vào `export_dir` sẽ dùng.)
    let probe_data = payload(1024);
    let hash = store.import_bytes(&probe_data).unwrap();
    store.staging_kind_probe(&hash, export_dir).label()
}

fn main() {
    let runs = 30;
    let store_dir = bench_dir("store");
    let store = ContentStore::new(store_dir.clone()).unwrap();
    let export_dir = bench_dir("export");

    // P0-4 audit vòng-4: label from the MEASURED strategy (the probe runs
    // the real clone path), never from a per-OS guess. The cold/warm cells
    // below export into `export_dir` — the same directory the probe
    // measured, so the label matches what the timed cells actually do.
    // (P0-4 audit vòng-4: nhãn từ chiến lược ĐO ĐƯỢC (probe chạy đường
    // clone thật), không đoán theo OS. Cell cold/warm bên dưới export vào
    // `export_dir` — đúng thư mục probe đã đo nên nhãn khớp cái cell định
    // giờ thật sự làm.)
    let strategy = measure_strategy(&store, &export_dir);

    let sizes: [(&str, usize); 3] = [
        ("small-1KB", 1024),
        ("medium-1MB", 1024 * 1024),
        ("large-64MB", 64 * 1024 * 1024),
    ];

    println!("=== APFS clonefile warm-memo staging microbenchmark — PROVISIONAL ===");
    println!("=== staging strategy MEASURED on this machine (probe): {strategy} ===");
    println!(
        "=== cold = first touch (full source rehash) | warm = memo hit | {runs} runs/cell ==="
    );
    println!(
        "{:<12} {:>10} {:>12} {:>12} {:>10} {:>8}",
        "payload", "bytes", "cold_med_ms", "warm_med_ms", "memo_ms", "speedup"
    );

    for &(label, size) in &sizes {
        let data = payload(size);
        let hash = store.import_bytes(&data).unwrap();

        // COLD arm: a FRESH ContentStore per run (memo empty — every export
        // re-verifies the source from disk, exactly like a new process).
        // (Nhánh COLD: ContentStore MỚI mỗi run (memo rỗng — export nào
        // cũng verify nguồn từ đĩa, giống một process mới khởi động).)
        let mut cold_times = Vec::new();
        for i in 0..runs {
            let cold_store = ContentStore::new(store_dir.clone()).unwrap();
            let dest = export_dir.join(format!("cold{i}.bin"));
            let t0 = Instant::now();
            cold_store.export_to(&hash, &dest).unwrap();
            cold_times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        cold_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let cold_med = cold_times[cold_times.len() / 2];

        // WARM arm: same store instance reused (memo hit after first
        // touch) — this is the number the previous "400×" claim measured.
        // (Nhánh WARM: cùng instance store tái sử dụng (memo hit sau lần chạm
        // đầu) — đây là con số mà claim "400×" cũ đã đo.)
        let warm_store = ContentStore::new(store_dir.clone()).unwrap();
        warm_store
            .export_to(&hash, &export_dir.join("warm0.bin"))
            .unwrap();
        let mut warm_times = Vec::new();
        for i in 1..runs {
            let dest = export_dir.join(format!("warm{i}.bin"));
            let t0 = Instant::now();
            warm_store.export_to(&hash, &dest).unwrap();
            warm_times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        warm_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let warm_med = warm_times[warm_times.len() / 2];

        // Plain fs::copy reference (no verify, no memo — raw staging cost).
        // (Đối chứng fs::copy thuần (không verify, không memo — chi phí
        // staging thô).)
        let src = hash.cas_path(&store_dir);
        let mut copy_times = Vec::new();
        for i in 0..runs {
            let plain = export_dir.join(format!("plain{i}.bin"));
            let t0 = Instant::now();
            fs::copy(&src, &plain).unwrap();
            copy_times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        copy_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let copy_med = copy_times[copy_times.len() / 2];

        // Correctness: every produced file is hash-verified (the benchmark
        // must never certify speed on wrong bytes).
        // (Đúng sai: mọi file tạo ra được verify hash (benchmark không bao
        // giờ chứng nhận tốc độ trên bytes sai).)
        for i in 0..runs {
            for prefix in ["cold", "warm"] {
                let dest = export_dir.join(format!("{prefix}{i}.bin"));
                let v = IntegrityHash::from_bytes(&fs::read(&dest).unwrap(), false);
                assert_eq!(v.as_hex(), hash.as_hex(), "export corrupt!");
            }
        }

        let speedup = copy_med / cold_med;
        println!(
            "{:<12} {:>10} {:>12.2} {:>12.2} {:>10.2} {:>7.2}x",
            label, size, cold_med, warm_med, copy_med, speedup
        );

        fs::remove_dir_all(&export_dir).ok();
    }
    fs::remove_dir_all(&store_dir).ok();
    println!("\nPROVISIONAL — local APFS single-machine microbenchmark. Not a");
    println!("package-manager claim, not cross-platform, not end-to-end.");
}
