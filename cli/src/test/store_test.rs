#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Unit tests for the staging-lease classifier (Gate 11-B.2, Task C).
//! (Test đơn vị cho classifier lease staging — 5 disposition.)

use super::*;

/// Current UNIX seconds — shared by the pid/lease-liveness tests.
/// (Giây UNIX hiện tại — dùng chung cho test pid/lease-còn-sống.)
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// LIVE via a fresh pid + fresh lease: the test process's OWN pid is alive,
/// so the Unix pid probe (`kill(pid, 0)`) must return true and the classifier
/// must not count an in-flight install as stale.
/// (LIVE qua pid tươi + lease tươi: pid của chính test process còn sống, nên
/// pid probe Unix trả true và classifier không được tính install đang bay là stale.)
#[cfg(unix)]
#[test]
fn classify_staging_lease_live_via_fresh_pid_lease() {
    let locks_root = tempfile::tempdir().unwrap();
    let now = now_secs();
    let lease = mgc_store::StagingLease {
        project_root: "/proj/live-pid".to_string(),
        generation: 1,
        lease_pid: Some(std::process::id() as i64),
        lease_started_at: Some(now),
        claim_count: 0,
    };
    let covered = std::collections::HashSet::new();
    let got = classify_staging_lease(&lease, &covered, locks_root.path(), now, 86_400);
    assert_eq!(got, StagingDisposition::Live);
}

/// LIVE via a held project install lock: with a REAL `ProjectInstallLock`
/// held, the classifier must return Live even with no pid and no lease —
/// the lock is the cross-platform liveness proof (P2-1).
/// (LIVE qua install lock project bị giữ: với `ProjectInstallLock` THẬT đang
/// giữ, classifier phải trả Live kể cả không pid lẫn lease — lock là bằng
/// chứng sống cross-platform (P2-1).)
#[test]
fn classify_staging_lease_live_via_held_install_lock() {
    let locks_root = tempfile::tempdir().unwrap();
    let project_root = "/proj/live-locked".to_string();
    // Hold the lock for the whole test — acquire_at below must then fail.
    // (Giữ lock suốt test — acquire_at bên dưới phải thất bại.)
    let _held =
        mgc_store::ProjectInstallLock::acquire_at(locks_root.path(), &project_root).unwrap();
    let lease = mgc_store::StagingLease {
        project_root,
        generation: 1,
        lease_pid: None,
        lease_started_at: None,
        claim_count: 0,
    };
    let covered = std::collections::HashSet::new();
    let got = classify_staging_lease(&lease, &covered, locks_root.path(), now_secs(), 86_400);
    assert_eq!(got, StagingDisposition::Live);
}

/// STALE: dead pid, fresh lease, zero claims — a leaked `begin` with no
/// claims is garbage and fails health until repaired.
/// (STALE: pid chết, lease tươi, không claim — begin rò không claim là rác
/// và làm hỏng health cho tới khi repair.)
#[test]
fn classify_pure_stale_claimless_dead_pid() {
    let got = classify_staging_lease_pure(0, false, true, false, false);
    assert_eq!(got, StagingDisposition::Stale);
}

/// STALE: dead pid, ancient lease, claims present but ALL covered by a
/// promoted generation — redundant protection, retirable without data loss.
/// (STALE: pid chết, lease cổ, có claim nhưng ĐỀU được promoted generation
/// giữ — bảo vệ thừa, nghỉ hưu được không mất dữ liệu.)
#[test]
fn classify_pure_stale_claims_covered_by_promoted() {
    let got = classify_staging_lease_pure(3, false, false, false, true);
    assert_eq!(got, StagingDisposition::Stale);
}

/// RETAINED: dead pid, claims present but NOT covered by any promoted
/// generation — the claim is that blob's only protection, so over-retention
/// is SAFE and must NOT fail health.
/// (RETAINED: pid chết, có claim nhưng KHÔNG được promoted generation nào
/// giữ — claim là bảo vệ DUY NHẤT của blob, giữ thừa AN TOÀN, KHÔNG hỏng.)
#[test]
fn classify_pure_retained_claims_not_covered() {
    let got = classify_staging_lease_pure(3, false, false, false, false);
    assert_eq!(got, StagingDisposition::Retained);
}
