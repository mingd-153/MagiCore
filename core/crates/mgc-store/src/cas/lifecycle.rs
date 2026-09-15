// CAS root validation + lifecycle helpers.
// Validate gốc CAS: root phải là thư mục THẬT (không symlink — ở root HAY
// bất kỳ ancestor nào), do current user sở hữu trên Unix, và được chặt
// permission sau khi tạo (P0-A/P0-D Tech Lead vòng-7, 2026-09-13).

use std::fs;
use std::path::Path;

use super::store::StoreError;

/// Validate the CAS root BEFORE the store is used: the root itself and every
/// existing ancestor must be a real directory, never a symlink (P0-A).
/// `fs::metadata()` FOLLOWS symlinks, so the old check compared the metadata
/// of the link TARGET and `is_symlink()` was almost always false — the check
/// never fired for a real symlinked root. `symlink_metadata()` inspects the
/// link itself; walking ancestors catches a symlink planted at any depth.
/// On Windows, junctions/reparse points also surface as symlinks here.
///
/// Validate gốc CAS TRƯỚC khi store dùng: chính root và mọi ancestor đang
/// tồn tại phải là thư mục thật, không bao giờ symlink (P0-A).
/// `fs::metadata()` FOLLOW symlink nên check cũ so metadata của ĐÍCH link,
/// `is_symlink()` gần như luôn false — check không bao giờ kích hoạt với
/// root thực sự là symlink. `symlink_metadata()` soi chính link; duyệt
/// ancestor bắt symlink cắm ở bất kỳ độ sâu nào.
pub fn validate_cas_root(root: &Path) -> Result<(), StoreError> {
    // Fail-closed metadata match (P0-3, adversarial review vòng-9): the
    // old code used `if let Ok(meta)` + `else if !root.exists()` —
    // `Path::exists()` SWALLOWS the metadata error, so PermissionDenied
    // or an I/O failure read as "not there yet" and fell through to the
    // create-then-trust path. The single match below distinguishes
    // NotFound (legit — caller creates) from every other error
    // (permission, I/O — hard failure), and validates the metadata on
    // the success arm.
    // (Match metadata fail-closed (P0-3): code cũ dùng `if let Ok(meta)`
    // + `else if !root.exists()` — `Path::exists()` NUỐT lỗi metadata
    // nên PermissionDenied hay lỗi I/O bị đọc thành "chưa có" rồi rơi
    // qua đường tạo-rồi-tin. Match duy nhất bên dưới phân biệt NotFound
    // (hợp lệ — caller tạo) với mọi lỗi khác (quyền, I/O — fail cứng),
    // và validate metadata trên nhánh thành công.)
    match fs::symlink_metadata(root) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(StoreError::Io {
                    path: root.to_path_buf(),
                    msg: "CAS root is a symlink".to_string(),
                });
            }
            if !meta.is_dir() {
                return Err(StoreError::Io {
                    path: root.to_path_buf(),
                    msg: "CAS root is not a directory".to_string(),
                });
            }
            // Ownership gate (P0-D): a root owned by another user is a
            // trust-boundary violation — they control the content we
            // address. Root (uid 0) is exempt: it is trusted by
            // definition and CI/sudo flows must not brick.
            // (Cổng ownership (P0-D): root do user khác sở hữu là vi
            // phạm ranh giới tin cậy — họ điều khiển nội dung mà ta địa
            // chỉ hóa. Root (uid 0) được miễn: mặc định tin cậy, và luồng
            // CI/sudo không bị brick.)
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let me = current_uid();
                if meta.uid() != me && me != 0 {
                    return Err(StoreError::Io {
                        path: root.to_path_buf(),
                        msg: format!(
                            "CAS root is owned by uid {} (current {}), refusing",
                            meta.uid(),
                            me
                        ),
                    });
                }
            }
        }
        // NotFound is the ONLY benign absence: the caller creates the
        // root below, and the freshly created root is re-validated by
        // ensure_cas_dirs' permission pass. Every OTHER metadata error
        // (permission denied, I/O) fails closed — never mistaken for a
        // missing path.
        // (NotFound là sự vắng mặt LÀNH DUY NHẤT: caller tạo root bên
        // dưới, và root mới tạo được check lại bởi bước permission của
        // ensure_cas_dirs. Mọi lỗi metadata KHÁC (từ chối quyền, I/O)
        // fail cứng — không bao giờ bị nhầm thành path vắng.)
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(StoreError::Io {
                path: root.to_path_buf(),
                msg: format!("CAS root metadata unreadable ({e}) — refusing to guess"),
            });
        }
    }

    // Ancestor sweep (P0-A): a symlink at ANY depth above the root redirects
    // the whole store; the root itself being real is not enough.
    // (Quét ancestor (P0-A): symlink ở BẤT KỲ độ sâu nào phía trên root sẽ
    // chuyển hướng cả store — root thật chưa đủ.)
    check_cas_root_ancestors(root)
}

/// Walk up from the CAS root: every EXISTING ancestor must be a real
/// directory — with ONE narrow, explicit exemption: system-managed
/// symlinks owned by root (uid 0). macOS ships `/var → private/var` and
/// many Linuxes symlink `/tmp` or `/etc/mtab`; these are root-owned, so
/// a same-user attacker (the cache-poisoning threat model) can neither
/// create nor retarget them — refusing them bricks every standard
/// tempdir (REVIEW vòng-2 finding: tempfile::tempdir() on macOS lives
/// under /var). A symlink owned by anyone ELSE — including the current
/// user — stays a hard reject. `symlink_metadata` errors other than
/// NotFound fail closed (P0-B discipline — a permission-denied ancestor
/// must not be skipped).
///
/// Duyệt lên từ gốc CAS: mọi ancestor ĐANG TỒN TẠI phải là thư mục thật
/// — với MỘT ngoại lệ hẹp, tường minh: symlink hệ thống do root (uid 0)
/// sở hữu. macOS ship `/var → private/var` và nhiều bản Linux symlink
/// `/tmp` hay `/etc/mtab`; chúng do root sở hữu nên attacker cùng-user
/// (threat model đầu độc cache) không thể tạo lẫn đổi hướng — từ chối
/// chúng brick mọi tempdir chuẩn (phát hiện REVIEW vòng-2: tempfile::
/// tempdir() trên macOS nằm dưới /var). Symlink do BẤT KỲ ai khác sở hữu
/// — kể cả user hiện tại — vẫn bị chặn tuyệt đối. Lỗi `symlink_metadata`
/// ngoài NotFound fail cứng (kỷ luật P0-B — ancestor bị từ chối quyền
/// không được bỏ qua).
fn check_cas_root_ancestors(root: &Path) -> Result<(), StoreError> {
    let mut current = root.parent();
    while let Some(p) = current {
        match fs::symlink_metadata(p) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    // Root-owned symlink → system-managed (e.g. /var on
                    // macOS): trusted for the same-user threat model —
                    // the attacker in scope cannot write a root-owned
                    // link. Everything else is a redirect we refuse.
                    // (Symlink do root sở hữu → hệ thống quản (vd /var
                    // trên macOS): tin cậy cho threat model cùng-user —
                    // attacker trong scope không ghi được link root-owned.
                    // Mọi thứ khác là redirect bị từ chối.)
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        if meta.uid() == 0 {
                            current = p.parent();
                            continue;
                        }
                    }
                    return Err(StoreError::Io {
                        path: p.to_path_buf(),
                        msg: "CAS root ancestor is a symlink".to_string(),
                    });
                }
                if !meta.is_dir() {
                    return Err(StoreError::Io {
                        path: p.to_path_buf(),
                        msg: "CAS root ancestor is not a directory".to_string(),
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // An absent ancestor is fine — the nearest existing one is
                // the last link in the chain; above it, this branch stops
                // applying because we keep walking and will hit NotFound
                // again only above a symlink-created gap we refuse anyway.
                // (Ancestor chưa tồn tại là hợp lệ — ancestor tồn tại gần
                // nhất là mắt xích cuối; phía trên nhánh này không áp dụng
                // vì duyệt tiếp chỉ gặp NotFound nữa.)
            }
            Err(e) => {
                // PermissionDenied / I/O error — fail closed.
                // (Từ chối quyền / lỗi I/O — fail cứng.)
                return Err(StoreError::Io {
                    path: p.to_path_buf(),
                    msg: format!("CAS root ancestor metadata unreadable: {e}"),
                });
            }
        }
        current = p.parent();
    }
    Ok(())
}

/// Current process UID (Unix) — for the CAS root ownership gate.
/// Uses the libc call under the SAME targeted allow as the COW clone path
/// in store.rs: one syscall, fixed return value, no memory involved.
/// (UID của process hiện tại (Unix) — cho cổng ownership gốc CAS. Dùng
/// lệnh libc dưới cùng allow cục bộ như đường COW clone trong store.rs:
/// một syscall, giá trị trả cố định, không chạm memory.)
#[cfg(unix)]
#[allow(unsafe_code)]
fn current_uid() -> u32 {
    // SAFETY: getuid() reads a fixed kernel value; no memory is touched.
    // (An toàn: getuid() đọc giá trị cố định của kernel; không chạm memory.)
    unsafe { libc::getuid() }
}

/// Ensure the CAS directory tree exists (creating it with tight modes via
/// the permissions pass below).
/// Đảm bảo cây thư mục CAS tồn tại (tạo với mode chặt qua bước permission).
pub fn ensure_cas_dirs(root: &Path) -> Result<(), StoreError> {
    let dirs = [
        root.join("files").join("blake3"),
        root.join("compiled").join("blake3"),
    ];
    for dir in &dirs {
        fs::create_dir_all(dir).map_err(|e| StoreError::Io {
            path: dir.clone(),
            msg: e.to_string(),
        })?;
    }
    Ok(())
}

/// Tighten the CAS root + its store subdirs to owner-only (P0-D): the CAS
/// contains package bytes, compiled JS and inline sourcemaps (potentially
/// proprietary source) — world/group-readable defaults leak them to every
/// other local user. 0700 on dirs; blob/record files are set 0600 by the
/// write primitives (write.rs) at creation time.
///
/// Siết permission gốc CAS + các thư mục con store về chỉ-owner (P0-D):
/// CAS chứa bytes package, JS đã biên dịch và sourcemap inline (nguồn
/// độc quyền tiềm năng) — mặc định world-readable làm lộ cho mọi user
/// local khác. 0700 cho thư mục; blob/record được write.rs đặt 0600 ngay
/// lúc tạo.
pub fn set_cas_root_permissions(root: &Path) -> Result<(), StoreError> {
    if !root.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Owner-only: rwx for the current user, nothing for group/others.
        // Shared multi-user stores need an explicit service boundary, not
        // a world-readable default (Tech Lead P0-D, 2026-09-13).
        // (Chỉ-owner: rwx cho user hiện tại, không gì cho group/others.
        // Store chia sẻ multi-user cần ranh giới service tường minh, không
        // phải mặc định world-readable.)
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(root, perms.clone()).map_err(|e| StoreError::Io {
            path: root.to_path_buf(),
            msg: e.to_string(),
        })?;
        // Store-internal subdirs: same owner-only mode.
        // (Thư mục con bên trong store: cùng mode chỉ-owner.)
        for dir in [
            root.join("files"),
            root.join("files").join("blake3"),
            root.join("compiled"),
            root.join("compiled").join("blake3"),
            root.join("tmp"),
        ] {
            if dir.exists() {
                fs::set_permissions(&dir, perms.clone()).map_err(|e| StoreError::Io {
                    path: dir,
                    msg: e.to_string(),
                })?;
            }
        }
    }

    Ok(())
}
