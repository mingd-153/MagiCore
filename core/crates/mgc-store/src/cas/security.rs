// Path security checks for the CAS (symlink ancestry, traversal).
// Kiểm tra bảo mật đường dẫn cho CAS: symlink ancestor checked trên từng
// component GỐC (không canonicalize trước — canonicalize làm mất dấu symlink
// đã resolve), chặn traversal `..`, và LỖI METADATA fail-closed (P0-B).

use std::fs;
use std::path::{Component, Path};

use super::store::StoreError;

/// Check if any ancestor of the path is a symlink (potential TOCTOU attack).
/// Walks the ORIGINAL path components (no canonicalize first — that would
/// resolve symlinks away and hide the attack). The path itself may not exist
/// yet; the first existing ancestor is resolved and the remainder is checked
/// component by component.
///
/// Error contract (P0-B, Tech Lead vòng-7 2026-09-13): `NotFound` on the
/// FINAL component is legitimate (the destination is about to be created);
/// every OTHER metadata failure — PermissionDenied, I/O error, filesystem
/// inconsistency — fails CLOSED. The old `if let Ok(meta)` silently skipped
/// unreadable components: a permission wall in the middle of the path was
/// indistinguishable from a clean check, letting the walk continue past it.
///
/// Kiểm tra ancestor của path có phải symlink không (chống TOCTOU). Duyệt
/// theo component GỐC của path (KHÔNG canonicalize trước — canonicalize sẽ
/// resolve symlink và che mất dấu tấn công). Path chưa tồn tại cũng được:
/// ancestor đầu tiên tồn tại được resolve rồi phần còn lại check từng component.
///
/// Hợp đồng lỗi (P0-B): `NotFound` ở component CUỐI là hợp lệ (dest sắp
/// được tạo); mọi lỗi metadata KHÁC — PermissionDenied, lỗi I/O, inconsistent
/// filesystem — fail CỨNG. `if let Ok(meta)` cũ bỏ qua component không đọc
/// được: bức tường permission giữa path không phân biệt được với check
/// sạch, để vòng lặp đi tiếp qua nó.
pub fn check_symlink_ancestors(path: &Path) -> Result<(), StoreError> {
    let mut current = Some(path);

    while let Some(p) = current {
        // The final component may not exist yet (dest being created) — only
        // existing components can carry symlink metadata. Any error other
        // than NotFound on a component we must inspect fails closed.
        // (Component cuối có thể chưa tồn tại (dest sắp tạo) — chỉ component
        // đã tồn tại mới có thể mang symlink metadata. Lỗi ngoài NotFound
        // trên component phải soi thì fail cứng.)
        match fs::symlink_metadata(p) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(StoreError::Io {
                        path: p.to_path_buf(),
                        msg: "symlink detected in path ancestry".to_string(),
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Not created yet — nothing to inspect here; keep walking up.
                // (Chưa được tạo — không có gì để soi ở đây; tiếp tục đi lên.)
            }
            Err(e) => {
                // PermissionDenied / I/O error / inconsistency: fail closed
                // — a boundary we cannot READ is not a boundary we verified.
                // (Từ chối quyền / lỗi I/O / inconsistent: fail cứng — ranh
                // giới KHÔNG ĐỌC ĐƯỢC không phải ranh giới đã verify.)
                return Err(StoreError::Io {
                    path: p.to_path_buf(),
                    msg: format!("path ancestry metadata unreadable: {e}"),
                });
            }
        }
        current = p.parent();
    }

    Ok(())
}

/// Check that the path does not contain `..` components (path traversal).
/// Chống path chứa component `..` (traversal).
pub fn check_path_traversal(path: &Path) -> Result<(), StoreError> {
    for component in path.components() {
        if let Component::ParentDir = component {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "path contains parent directory traversal (..)".to_string(),
            });
        }
    }
    Ok(())
}
