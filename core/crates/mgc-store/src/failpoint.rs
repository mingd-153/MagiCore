//! Test-only failpoint injection for deterministic crash-recovery testing
//! (Gate 11-B.2, vòng-11). A `hit()` call is an env-gated NO-OP unless the
//! caller has set `MGC_FAILPOINT`; when that env var names the calling
//! phase, the process writes a `READY:<phase>` marker (fsync) and parks
//! forever until an external harness SIGKILLs it.
//!
//! (Tiêm điểm lỗi chỉ-cho-test để test phục hồi crash tất định. Lời gọi
//! `hit()` là NO-OP gated theo env trừ khi caller đặt `MGC_FAILPOINT`;
//! khi env đó trỏ đúng phase, process ghi marker `READY:<phase>` (fsync)
//! và đỗ vĩnh viễn tới khi harness ngoài SIGKILL nó.)

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Every failpoint phase the install orchestrator exposes. Kept in ONE
/// list so a misspelled `MGC_FAILPOINT` value can be detected and warned
/// about instead of silently running the whole install to completion.
/// (Mọi phase failpoint mà orchestrator install phơi bày. Gộp MỘT danh
/// sách để giá trị `MGC_FAILPOINT` gõ sai bị phát hiện và cảnh báo, thay
/// vì âm thầm chạy trọn install tới xong.)
const KNOWN_PHASES: &[&str] = &[
    "after-generation-begin",
    "after-first-claim",
    "after-fetch",
    "after-cas-publish",
    "before-materialize",
    "after-materialize",
    "before-rollback-restore",
    "before-promote",
    "after-generation-flip",
    "before-commit",
    "after-commit",
];

/// Sleep interval while parked — the process is deliberately stuck here
/// until killed; a long single sleep keeps the hot loop cost negligible.
/// (Khoảng ngủ khi đỗ — process cố ý kẹt ở đây tới khi bị kill; ngủ dài
/// đơn giữ chi phí vòng lặp nóng không đáng kể.)
const PARK_SLEEP_SECS: u64 = 3_600;

/// Guards the once-only "unknown phase" warning across the many `hit()`
/// call sites (only the FIRST mismatch prints).
/// (Chặn cảnh báo "phase không biết" chỉ in MỘT lần qua nhiều chỗ gọi
/// `hit()` — chỉ lần khớp ĐẦU TIÊN in.)
static WARNED_UNKNOWN_PHASE: AtomicBool = AtomicBool::new(false);

/// Fire the failpoint named `phase`.
/// (Kích hoạt failpoint tên `phase`.)
///
/// # Contract (Hợp đồng)
/// - No `MGC_FAILPOINT` set → returns immediately (one `env::var` read,
///   no observable behavior change for production).
///   (Chưa đặt `MGC_FAILPOINT` → trả về ngay (một lần đọc `env::var`,
///   không thay đổi hành vi production quan sát được).)
/// - `MGC_FAILPOINT == phase` → write `READY:<phase>\n` to the file named
///   by `MGC_FAILPOINT_READY_FILE` (fsync so the parent reliably sees it),
///   then park forever — NO exit, NO panic; the process stays alive until
///   SIGKILL.
///   (`MGC_FAILPOINT == phase` → ghi `READY:<phase>\n` vào file theo
///   `MGC_FAILPOINT_READY_FILE` (fsync để parent chắc chắn thấy), rồi đỗ
///   vĩnh viễn — KHÔNG exit, KHÔNG panic; process sống tới khi SIGKILL.)
/// - `MGC_FAILPOINT` set to an unknown phase → warn once in English, then
///   run normally (a typo must not fail a real install).
///   (`MGC_FAILPOINT` trỏ phase không biết → cảnh báo một lần tiếng Anh,
///   rồi chạy bình thường — gõ sai không được làm hỏng install thật.)
///
/// # Safety note (Ghi chú an toàn)
/// This is a TEST-ONLY hook: whoever controls the process environment
/// already controls the process, so parking here grants no capability the
/// caller did not already have.
/// (Đây là hook CHỈ-CHO-TEST: ai kiểm soát env của process đã kiểm soát
/// process, nên đỗ ở đây không trao thêm khả năng nào caller chưa có.)
pub fn hit(phase: &str) {
    // No failpoint configured → the common production path, cost ~1 env read.
    // (Chưa cấu hình failpoint → đường production thường gặp, chi phí ~1 lần đọc env.)
    let Some(target) = std::env::var("MGC_FAILPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return;
    };

    if target == phase {
        // Signal readiness to the harness, then park until killed.
        // (Báo hiệu sẵn sàng cho harness, rồi đỗ tới khi bị kill.)
        if let Ok(path) = std::env::var("MGC_FAILPOINT_READY_FILE")
            && let Ok(mut file) = std::fs::File::create(&path)
        {
            // fsync via sync_all so the polling parent observes the marker,
            // not a still-buffered write.
            // (fsync qua sync_all để parent đang poll quan sát được marker,
            // không phải lần ghi còn đệm.)
            let _ = file.write_all(format!("READY:{phase}\n").as_bytes());
            let _ = file.sync_all();
        }
        loop {
            std::thread::sleep(Duration::from_secs(PARK_SLEEP_SECS));
        }
    }

    // A mismatched-but-unknown target is a typo — surface it exactly once.
    // (Target khác phase nhưng không phải phase hợp lệ nào là gõ sai — nêu đúng một lần.)
    if !KNOWN_PHASES.contains(&target.as_str())
        && !WARNED_UNKNOWN_PHASE.swap(true, Ordering::Relaxed)
    {
        eprintln!(
            "WARNING: MGC_FAILPOINT '{target}' matches no known failpoint phase \
             (typo?); install runs normally"
        );
    }
}
