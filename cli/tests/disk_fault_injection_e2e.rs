//! REAL disk-fault injection E2E (Gate 11-B remaining-work items):
//!
//! 1. ENOSPC — a REAL 10 MB APFS volume (hdiutil, macOS) holds the
//!    store. A healthy baseline install runs first; the volume is then
//!    filled to ZERO free bytes; the re-install must FAIL CLEANLY
//!    (typed English error, exit 1, no panic) with the store left
//!    reparable; after freeing space the follow-up install SUCCEEDS and
//!    the doctor is fully HEALTHY (0 orphan / 0 missing / 0 corrupt /
//!    0 stale staging) — a mid-transaction disk-full must never commit
//!    a half-state. This is the REAL "disk-full between BEGIN and
//!    COMMIT" falsification the structural rollback review could not
//!    provide.
//! 2. Permission injection (Unix chmod) — a read-only store dir blocks
//!    the re-install cleanly (typed permission error), and a read-only
//!    node_modules blocks materialize cleanly; both recover with a
//!    follow-up install + doctor HEALTHY. Windows file-locked targets
//!    are a documented platform limitation (no POSIX chmod there — the
//!    pid-probe/install-lock notes in the doctor cover the Windows
//!    liveness side).
//!
//! (E2E bơm lỗi đĩa THẬT (các mục còn mở của Gate 11-B):
//! 1. ENOSPC — volume APFS THẬT 10 MB (hdiutil, macOS) chứa store.
//!    Install baseline khỏe chạy trước; volume bị nhồi ĐẾN 0 byte
//!    trống; install-lại phải THẤT BẠI SẠCH (lỗi tiếng Anh có type,
//!    exit 1, không panic) với store còn sửa được; sau khi nhả chỗ,
//!    install kế tiếp THÀNH CÔNG và doctor HEALTHY đầy đủ (0 orphan/
//!    missing/corrupt/stale staging) — disk-full giữa transaction
//!    không bao giờ được commit nửa vời. Đây là câu hỏi phản bác
//!    "disk-full giữa BEGIN↔COMMIT" THẬT mà review cấu trúc không
//!    đưa được.
//! 2. Bơm permission (Unix chmod) — store dir read-only chặn
//!    install-lại sạch (lỗi permission có type); node_modules
//!    read-only chặn materialize sạch; cả hai hồi phục bằng install
//!    kế tiếp + doctor HEALTHY. File-locked trên Windows là giới hạn
//!    platform được ghi nhận (không có POSIX chmod — phần pid-probe/
//!    install-lock của doctor đã che phía liveness Windows).)

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    std::env::var("CARGO_BIN_EXE_mgc")
        .expect("CARGO_BIN_EXE_mgc not set — run via `cargo test -p mgc`")
}

fn create_web_project(temp: &Path, name: &str) -> PathBuf {
    let project = temp.join(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{ "name": "victim", "version": "1.0.0", "dependencies": { "is-odd": "3.0.1" } }"#,
    )
    .unwrap();
    std::fs::write(project.join(".mgc.core"), "web\n").unwrap();
    project
}

fn install_cmd(mgc: &str, project: &Path, registry_url: &str, cache_dir: &Path) -> Command {
    let mut cmd = Command::new(mgc);
    cmd.arg("--core").arg("web").arg("install");
    cmd.current_dir(project);
    cmd.env("MAGICORE_WEB_REGISTRY_URL", registry_url);
    cmd.env("MAGICORE_WEB_ALLOWED_REGISTRIES", registry_url);
    cmd.env("MGC_CACHE_DIR", cache_dir);
    cmd
}

fn doctor_cmd(mgc: &str, project: &Path, cache_dir: &Path) -> Command {
    let mut cmd = Command::new(mgc);
    cmd.arg("store").arg("doctor").arg("--core").arg("web");
    cmd.current_dir(project);
    cmd.env("MGC_CACHE_DIR", cache_dir);
    cmd
}

fn assert_doctor_fully_healthy(mgc: &str, project: &Path, cache_dir: &Path, context: &str) {
    let doctor = doctor_cmd(mgc, project, cache_dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        doctor.status.success(),
        "{context}: doctor must be HEALTHY:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert!(stdout.contains("0 orphan claim(s)"), "{context}: {stdout}");
    assert!(
        stdout.contains("0 stale staging gen(s)"),
        "{context}: {stdout}"
    );
    assert!(stdout.contains("0 missing blob(s)"), "{context}: {stdout}");
    assert!(stdout.contains("0 corrupt blob(s)"), "{context}: {stdout}");
}

// ─── 1. ENOSPC (macOS real volume) ──────────────────────────────────────────

/// RAII detach guard — the volume is force-ejected on every exit path.
/// (Guard detach RAII — volume bị force-eject trên mọi đường exit.)
#[cfg(target_os = "macos")]
struct VolumeGuard {
    mountpoint: PathBuf,
}

#[cfg(target_os = "macos")]
impl Drop for VolumeGuard {
    fn drop(&mut self) {
        let _ = Command::new("hdiutil")
            .arg("detach")
            .arg(&self.mountpoint)
            .arg("-force")
            .output();
    }
}

#[cfg(target_os = "macos")]
fn run(cmd: &mut Command, context: &str) -> std::process::Output {
    // NOTE: std's Command builder methods (`arg`) return `&mut Command`
    // — the run() signature follows that shape so chained calls and
    // by-value Command both flow through with a `&mut` temporary.
    // (LƯU Ý: các builder method của std (`arg`) trả `&mut Command` —
    // signature run() theo đúng shape đó để chuỗi chain lẫn Command
    // by-value đều chảy qua bằng `&mut` tạm thời.)
    cmd.output()
        .unwrap_or_else(|e| panic!("{context}: spawn failed: {e}"))
}

/// REAL ENOSPC: fill a 50 MB volume that holds the store; re-install
/// fails cleanly; recovery is complete.
/// (ENOSPC THẬT: nhồi volume 50 MB chứa store; install-lại thất bại
/// sạch; phục hồi hoàn toàn.)
#[cfg(target_os = "macos")]
#[test]
fn enospc_on_the_store_volume_fails_clean_and_recovers() {
    let mgc = find_mgc_binary();
    let scratch = TempDir::new().unwrap();
    let dmg_path = scratch.path().join("mgc-enospc-e2e.dmg");
    let mountpoint = scratch.path().join("vol");
    std::fs::create_dir_all(&mountpoint).unwrap();

    // Unique volume name (pid-suffixed) — parallel test binaries never
    // collide on /Volumes.
    // (Tên volume duy nhất (hậu tố pid) — binary test song song không
    // bao giờ đụng nhau trên /Volumes.)
    let volname = format!("mgc-enospc-{}", std::process::id());

    // 50 MB: big enough for the baseline store (~100 KB), small enough
    // to fill in seconds. 10 MB was too small for APFS to reliably
    // report ENOSPC on write.
    // (50 MB: đủ lớn cho store baseline (~100 KB), đủ nhỏ để nhồi trong
    // vài giây. 10 MB quá nhỏ để APFS báo ENOSPC tin cậy khi ghi.)
    let out = run(
        Command::new("hdiutil")
            .arg("create")
            .arg("-size")
            .arg("50m")
            .arg("-fs")
            .arg("APFS")
            .arg("-volname")
            .arg(&volname)
            .arg(&dmg_path),
        "hdiutil create",
    );
    assert!(
        out.status.success(),
        "hdiutil create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run(
        Command::new("hdiutil")
            .arg("attach")
            .arg(&dmg_path)
            .arg("-mountpoint")
            .arg(&mountpoint)
            .arg("-nobrowse"),
        "hdiutil attach",
    );
    assert!(
        out.status.success(),
        "hdiutil attach failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _guard = VolumeGuard {
        mountpoint: mountpoint.clone(),
    };

    // Registry fixture: inline mockito (same shape as the race E2E).
    // (Registry fixture: mockito inline (cùng hình dáng E2E race).)
    let mut server = mockito::Server::new();
    let url = server.url();
    let metadata = serde_json::json!({
        "name": "is-odd",
        "dist-tags": { "latest": "3.0.1" },
        "versions": { "3.0.1": {
            "name": "is-odd",
            "version": "3.0.1",
            "dependencies": {},
            "dist": { "tarball": format!("{url}/is-odd/-/is-odd-3.0.1.tgz") }
        }}
    });
    let _metadata_mock = server
        .mock("GET", "/is-odd")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(metadata.to_string())
        .expect_at_least(1)
        .create();
    let _tarball_mock = server
        .mock("GET", "/is-odd/-/is-odd-3.0.1.tgz")
        .with_status(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(make_tarball())
        .expect_at_least(1)
        .create();

    // Create project INSIDE the mounted volume so the store (project/.magicore/cache/web)
    // lives on the same volume — otherwise ENOSPC never hits the real store.
    // (Tạo project BÊN TRONG volume mount để store (project/.magicore/cache/web)
    // nằm trên cùng volume — nếu không ENOSPC không bao giờ chạm store thật.)
    let project = create_web_project(&mountpoint, "site");
    // MGC_CACHE_DIR = project root's .magicore → web adapter uses project_cache_dir
    // = project/.magicore/cache/web → which IS on the mounted volume.
    let cache_dir = project.join(".magicore");

    // Baseline: a healthy install POPULATES the store on the small
    // volume (db + WAL + CAS blobs all live there).
    // (Baseline: install khỏe LẤP ĐẦY store trên volume nhỏ (db + WAL +
    // blob CAS đều nằm đó).)
    let out = run(
        &mut install_cmd(&mgc, &project, &url, &cache_dir),
        "baseline install",
    );
    assert!(
        out.status.success(),
        "baseline install on the volume must succeed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Fill the volume to ZERO free bytes by writing a SINGLE large
    // file sequentially — this is far more reliable than many small
    // files (less metadata overhead, no compression/dedup of zeros).
    // Write incompressible data until the write fails, then a 1-byte
    // probe MUST fail too. Flush to disk after each chunk so the OS
    // actually allocates blocks — otherwise buffered writes succeed
    // in memory but ENOSPC isn't hit until flush.
    // (Nhồi volume đến 0 byte trống bằng cách ghi MỘT file LỚN lần lượt
    // — tin cậy hơn nhiều so với nhiều file nhỏ (ít metadata overhead,
    // không nén/dedup số 0). Ghi dữ liệu không nén tới khi ghi lỗi,
    // rồi probe 1 byte PHẢI lỗi theo. Flush xuống đĩa sau mỗi chunk để
    // OS thực sự cấp phát block — nếu không write buffer trong memory
    // thành công nhưng ENOSPC chỉ nổ lúc flush.)
    let filler_file = mountpoint.join("filler.dat");
    use rand::RngCore;
    let mut rng = rand::rng();
    let chunk: Vec<u8> = (0..65536).map(|_| rng.next_u32() as u8).collect();
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&filler_file).unwrap();
        loop {
            if f.write_all(&chunk).is_err() {
                break;
            }
            let _ = f.flush();
            let _ = f.sync_all();
        }
    }
    // probe must WRITE (a 0-byte File::create can still succeed on a
    // full APFS volume — ENOSPC fires on block allocation, not open).
    // (probe phải GHI — File::create 0-byte vẫn có thể thành công trên
    // volume APFS đầy — ENOSPC nổ lúc cấp phát block, không phải lúc
    // open.)
    let probe_write = std::fs::File::create(mountpoint.join("probe"))
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"x"));
    assert!(
        probe_write.is_err(),
        "after filling, a 1-byte probe WRITE must FAIL (ENOSPC not reached otherwise)"
    );

    // Force the re-install to do REAL store work: remove node_modules
    // AND the tarball cache — the early-return "everything already
    // installed" path and warm-tarball dedup never touch the store and
    // would mask the ENOSPC fault.
    // (Buộc install-lại làm công việc store THẬT: xóa node_modules VÀ
    // cache tarball — đường early-return "mọi thứ đã cài" lẫn dedup
    // tarball nóng không bao giờ chạm store và sẽ che lỗi ENOSPC.)
    std::fs::remove_dir_all(project.join("node_modules")).unwrap();
    let tarball_cache = cache_dir.join("web").join("cache");
    if tarball_cache.exists() {
        std::fs::remove_dir_all(&tarball_cache).unwrap();
    }

    // Re-install under ENOSPC: exit 1, typed English error, no panic.
    // (Install-lại dưới ENOSPC: exit 1, lỗi tiếng Anh có type, không
    // panic.)
    let out = run(
        &mut install_cmd(&mgc, &project, &url, &cache_dir),
        "enospc install",
    );
    assert!(
        !out.status.success(),
        "install with a FULL store volume must fail (ENOSPC)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Error:"),
        "the ENOSPC failure must surface a typed error chain, stderr:\n{stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "exit code must be the clean 1 (anyhow error), not a crash signal"
    );

    // Recovery: free the volume, follow-up install SUCCEEDS, doctor is
    // FULLY healthy — a mid-transaction ENOSPC left no half-state.
    // (Phục hồi: nhả volume, install kế tiếp THÀNH CÔNG, doctor HEALTHY
    // đầy đủ — ENOSPC giữa transaction không để lại trạng thái nửa vời.)
    let _ = std::fs::remove_file(&filler_file);
    let out = run(
        &mut install_cmd(&mgc, &project, &url, &cache_dir),
        "recovery install",
    );
    assert!(
        out.status.success(),
        "follow-up install after freeing space must succeed:\n{}\n--- previous enospc stderr:\n{stderr}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_doctor_fully_healthy(&mgc, &project, &cache_dir, "post-recovery");
    assert!(
        project.join("node_modules/is-odd/package.json").exists(),
        "recovery install must materialize node_modules/is-odd"
    );
}

#[cfg(unix)]
fn make_tarball() -> Vec<u8> {
    let package_json = serde_json::json!({
        "name": "is-odd",
        "version": "3.0.1",
        "main": "index.js"
    })
    .to_string();
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(package_json.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "package/package.json", package_json.as_bytes())
        .unwrap();
    let index = "module.exports = function isOdd(n){return Math.abs(n % 2) === 1;};";
    let mut header = tar::Header::new_gnu();
    header.set_size(index.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "package/index.js", index.as_bytes())
        .unwrap();
    let encoder = archive.into_inner().unwrap();
    encoder.finish().unwrap()
}

// ─── 2. Permission injection (Unix chmod) ───────────────────────────────────

/// A read-only STORE dir blocks the re-install with a typed permission
/// error, then full recovery.
/// (Store dir read-only chặn install-lại bằng lỗi permission có type,
/// rồi phục hồi đầy đủ.)
#[cfg(unix)]
#[test]
fn readonly_store_dir_blocks_install_cleanly_and_recovers() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = TempDir::new().unwrap();
    let mgc = find_mgc_binary();

    let mut server = mockito::Server::new();
    let url = server.url();
    let metadata = serde_json::json!({
        "name": "is-odd",
        "dist-tags": { "latest": "3.0.1" },
        "versions": { "3.0.1": {
            "name": "is-odd",
            "version": "3.0.1",
            "dependencies": {},
            "dist": { "tarball": format!("{url}/is-odd/-/is-odd-3.0.1.tgz") }
        }}
    });
    let _metadata_mock = server
        .mock("GET", "/is-odd")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(metadata.to_string())
        .expect_at_least(1)
        .create();
    let _tarball_mock = server
        .mock("GET", "/is-odd/-/is-odd-3.0.1.tgz")
        .with_status(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(make_tarball())
        .expect_at_least(1)
        .create();

    let project = create_web_project(scratch.path(), "site");
    let cache_dir = project.join(".magicore");

    // Baseline healthy install.
    // (Baseline install khỏe.)
    let out = install_cmd(&mgc, &project, &url, &cache_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "baseline install failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read-only the store dir — both SQLite (WAL/journal) and CAS
    // writes must fail through it. node_modules AND tarball cache are
    // wiped FIRST so the early-return "already installed" path and
    // warm-tarball dedup never touch the store and would mask the
    // permission fault.
    // (Store dir read-only — cả ghi SQLite (WAL/journal) lẫn CAS đều
    // phải lỗi qua đó. node_modules VÀ cache tarball bị XÓA TRƯỚC để
    // đường early-return "đã cài xong" lẫn dedup tarball nóng không
    // nhảy qua phần việc store (nếu không nó sẽ che lỗi permission).)
    std::fs::remove_dir_all(project.join("node_modules")).unwrap();
    // The web adapter's store is at <MGC_CACHE_DIR>/cache/web (not just <MGC_CACHE_DIR>/web).
    // MGC_CACHE_DIR = project/.magicore → store_dir = project/.magicore/cache/web.
    let store_dir = cache_dir.join("cache").join("web");
    let tarball_cache = store_dir.join("cache");
    if tarball_cache.exists() {
        std::fs::remove_dir_all(&tarball_cache).unwrap();
    }
    assert!(store_dir.exists(), "baseline must create the web store dir");
    std::fs::set_permissions(&store_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let out = install_cmd(&mgc, &project, &url, &cache_dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "re-install with a read-only store MUST fail, stderr:\n{stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "exit must be the clean typed-error code, not a crash signal"
    );

    // Restore and prove full recovery.
    // (Phục hồi quyền và chứng minh hồi phục đầy đủ.)
    std::fs::set_permissions(&store_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = install_cmd(&mgc, &project, &url, &cache_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "follow-up install after restoring permissions must succeed:\n{stderr}\n---\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_doctor_fully_healthy(&mgc, &project, &cache_dir, "post-permission-recovery");
}

/// A read-only node_modules blocks MATERIALIZATION cleanly, then full
/// recovery — the fault lands between extract and promote.
/// (node_modules read-only chặn MATERIALIZATION sạch rồi phục hồi đầy
/// đủ — lỗi rơi giữa extract và promote.)
#[cfg(unix)]
#[test]
fn readonly_node_modules_blocks_materialize_cleanly_and_recovers() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = TempDir::new().unwrap();
    let mgc = find_mgc_binary();

    let mut server = mockito::Server::new();
    let url = server.url();
    let metadata = serde_json::json!({
        "name": "is-odd",
        "dist-tags": { "latest": "3.0.1" },
        "versions": { "3.0.1": {
            "name": "is-odd",
            "version": "3.0.1",
            "dependencies": {},
            "dist": { "tarball": format!("{url}/is-odd/-/is-odd-3.0.1.tgz") }
        }}
    });
    let _metadata_mock = server
        .mock("GET", "/is-odd")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(metadata.to_string())
        .expect_at_least(1)
        .create();
    let _tarball_mock = server
        .mock("GET", "/is-odd/-/is-odd-3.0.1.tgz")
        .with_status(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(make_tarball())
        .expect_at_least(1)
        .create();

    let project = create_web_project(scratch.path(), "site");
    let cache_dir = project.join(".magicore");

    let out = install_cmd(&mgc, &project, &url, &cache_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "baseline install failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read-only node_modules — the materialize rename INTO it must
    // fail. node_modules is wiped and re-created read-only; tarball
    // cache is also wiped so the installer MUST download and
    // materialize from scratch — the existing-tree early return would
    // otherwise skip materialization entirely and mask the fault.
    // (node_modules read-only — rename materialize VÀO đó phải lỗi.
    // node_modules bị XÓA và tạo lại ở chế độ read-only; cache tarball
    // cũng bị xóa để installer BẮT BUỘC tải và materialize từ đầu — đường
    // early-return trên cây đã có sẽ bỏ qua hẳn materialization và che
    // lỗi.)
    let node_modules = project.join("node_modules");
    assert!(node_modules.exists(), "baseline must create node_modules");
    std::fs::remove_dir_all(&node_modules).unwrap();
    // The web adapter's store is at <MGC_CACHE_DIR>/cache/web (not just <MGC_CACHE_DIR>/web).
    let tarball_cache = cache_dir.join("cache").join("web").join("cache");
    if tarball_cache.exists() {
        std::fs::remove_dir_all(&tarball_cache).unwrap();
    }
    std::fs::create_dir_all(&node_modules).unwrap();
    std::fs::set_permissions(&node_modules, std::fs::Permissions::from_mode(0o555)).unwrap();

    let out = install_cmd(&mgc, &project, &url, &cache_dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "re-install with a read-only node_modules MUST fail, stderr:\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "clean typed-error exit only");

    std::fs::set_permissions(&node_modules, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = install_cmd(&mgc, &project, &url, &cache_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "follow-up install after restoring node_modules must succeed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_doctor_fully_healthy(&mgc, &project, &cache_dir, "post-materialize-recovery");
}
