//! `scanners/bun_deno.rs` — Bun/Deno lockfile readers (P2 web parity,
//! Tech Lead 2026-09-10 matrix rows "Web JS/TS Node, Bun, Deno").
//!
//! Bun `bun.lock` (v1, JSONC) and Deno `deno.lock` (v5, JSON) both pin
//! npm-compatible packages. The readers extract ONLY the npm dep set
//! (name@version) so the SAME npm Bulk Advisory flow audits them — one
//! advisory pipeline for every JS runtime. JSR-only Deno deps carry no
//! npm advisory database mapping yet and are reported honestly as
//! skipped (never silently dropped).
//!
//! Đọc lockfile Bun/Deno: bun.lock (v1, JSONC) và deno.lock (v5, JSON)
//! đều ghim package tương thích npm. Bộ đọc chỉ trích tập dep npm
//! (name@version) để CÙNG một pipeline npm Bulk Advisory audit — một
//! đường advisory cho mọi runtime JS. Dep chỉ có JSR của Deno chưa có
//! mapping database advisory npm nên được báo trung thực là skipped
//! (không bao giờ bỏ âm thầm).

use mgc_types::{MgError, MgResult, strip_jsonc};
use serde::Deserialize;
use std::path::Path;

/// One npm package pin extracted from a Bun/Deno lockfile.
/// Một ghim package npm trích từ lockfile Bun/Deno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmPin {
    pub name: String,
    pub version: String,
}

/// A JSR-only dependency that cannot ride the npm advisory pipeline.
/// Dep chỉ có JSR — không đi được đường advisory npm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JrSkipped {
    pub specifier: String,
}

/// Result of reading a Bun/Deno lockfile: audit-ready npm pins plus the
/// honestly-skipped JSR set.
/// Kết quả đọc lockfile Bun/Deno: ghim npm sẵn audit + tập JSR bị bỏ
/// (được ghi rõ).
#[derive(Debug, Clone, Default)]
pub struct BunDenoRead {
    pub npm_pins: Vec<NpmPin>,
    pub jsr_skipped: Vec<JrSkipped>,
}

// ---------------------------------------------------------------------------
// Bun lockfile v1 (JSONC — trailing commas allowed by Bun's writer).
// packages: { "<name>": ["<name>@<version>", "", {...}, "<integrity>"] }
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct BunLock {
    #[serde(default)]
    packages: serde_json::Map<String, serde_json::Value>,
}

/// Read `bun.lock` (JSONC, trailing commas) into npm pins. Only entries
/// whose first array element parses as `name@version` are npm deps —
/// workspace/registry entries without that shape are skipped WITH a
/// recorded reason (never silently).
/// Đọc `bun.lock` (JSONC, dấu phẩy cuối) thành ghim npm. Chỉ entry có
/// phần tử mảng đầu parse được `name@version` là dep npm — entry khác
/// dạng bị bỏ KÈM lý do ghi rõ (không âm thầm).
pub fn read_bun_lock(raw: &str) -> MgResult<BunDenoRead> {
    // Strip JSONC: line comments and trailing commas before the JSON
    // parse. Bun writes trailing commas after the final array element
    // and object member — plain serde_json rejects them.
    // Cắt JSONC: comment dòng + dấu phẩy cuối trước khi parse JSON. Bun
    // ghi dấu phẩy cuối sau phần tử mảng cuối và member object cuối —
    // serde_json thuần từ chối.
    let cleaned = strip_jsonc(raw);
    let lock: BunLock = serde_json::from_str(&cleaned)
        .map_err(|e| MgError::Other(format!("invalid bun.lock: {e}")))?;

    let mut read = BunDenoRead::default();
    for (key, entry) in &lock.packages {
        // Entry shape: ["<pkg>@<version>", "", {deps}, "integrity"].
        // Dạng entry: ["<pkg>@<version>", "", {deps}, "integrity"].
        let Some(arr) = entry.as_array() else {
            return Err(MgError::Other(format!(
                "bun.lock package '{key}' entry must be an array"
            )));
        };
        let Some(first) = arr.first().and_then(|v| v.as_str()) else {
            return Err(MgError::Other(format!(
                "bun.lock package '{key}' first entry must be a name@version string"
            )));
        };
        match split_name_version(first) {
            Some((name, version)) => read.npm_pins.push(NpmPin { name, version }),
            None => {
                // Entries like "workspace:root" or registry-scoped ids are
                // not npm pins — record them as skipped with the raw id.
                // Entry kiểu "workspace:root" hoặc id registry-scoped
                // không phải ghim npm — ghi skipped kèm id thô.
                read.jsr_skipped.push(JrSkipped {
                    specifier: first.to_string(),
                });
            }
        }
    }
    Ok(read)
}

// ---------------------------------------------------------------------------
// Deno lockfile v5 (strict JSON).
// npm: { "lodash@4.17.20": { integrity } } — npm pins.
// jsr: { "@std/bytes@0.224.0": { integrity } } — JSR (no npm advisory DB).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DenoLock {
    #[serde(default)]
    npm: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    jsr: serde_json::Map<String, serde_json::Value>,
}

/// Read `deno.lock` (v5) into npm pins + honestly-skipped JSR deps.
/// Đọc `deno.lock` (v5) thành ghim npm + dep JSR bị bỏ (ghi rõ).
pub fn read_deno_lock(raw: &str) -> MgResult<BunDenoRead> {
    let lock: DenoLock =
        serde_json::from_str(raw).map_err(|e| MgError::Other(format!("invalid deno.lock: {e}")))?;

    let mut read = BunDenoRead::default();
    for pin in lock.npm.keys() {
        let Some((name, version)) = split_name_version(pin) else {
            return Err(MgError::Other(format!(
                "deno.lock npm pin '{pin}' must be name@version"
            )));
        };
        read.npm_pins.push(NpmPin { name, version });
    }
    for pin in lock.jsr.keys() {
        read.jsr_skipped.push(JrSkipped {
            specifier: pin.to_string(),
        });
    }
    Ok(read)
}

/// Split `name@version` (name may be @scope/x — split on the LAST '@').
/// Tách `name@version` (name có thể @scope/x — tách theo '@' CUỐI).
fn split_name_version(pin: &str) -> Option<(String, String)> {
    let at = pin.rfind('@')?;
    if at == 0 {
        // "@scope/pkg" without a version — not a pin.
        // "@scope/pkg" thiếu version — không phải ghim.
        return None;
    }
    let name = pin[..at].to_string();
    let version = pin[at + 1..].to_string();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version))
}

// The JSONC cleanup (quote-aware `//` comments + trailing commas) now
// lives in mgc-types::jsonc — ONE implementation shared by this scanner
// and the mgc-lockfile importer (P1 dedup, 2026-09-11).
// Phần dọn JSONC (comment `//` nhận-biết-quote + dấu phẩy cuối) giờ
// nằm ở mgc-types::jsonc — một bản dùng chung cho scanner này và
// importer của mgc-lockfile (dedup P1).

/// Discover + read the first JS-runtime lockfile present: mgc.lock is
/// the canonical one; bun.lock / deno.lock feed the same advisory flow
/// when mgc.lock has not been generated yet (P2 web parity).
/// Khám phá + đọc lockfile JS-runtime đầu tiên có mặt: mgc.lock là bản
/// chuẩn; bun.lock / deno.lock cùng vào đường advisory khi mgc.lock
/// chưa được sinh (parity web P2).
pub fn read_js_lockfiles(project_root: &Path) -> MgResult<BunDenoRead> {
    let mut combined = BunDenoRead::default();
    let bun_path = project_root.join("bun.lock");
    if bun_path.is_file() {
        let raw = std::fs::read_to_string(&bun_path)
            .map_err(|e| MgError::Other(format!("read bun.lock: {e}")))?;
        let read = read_bun_lock(&raw)?;
        combined.npm_pins.extend(read.npm_pins);
        combined.jsr_skipped.extend(read.jsr_skipped);
    }
    let deno_path = project_root.join("deno.lock");
    if deno_path.is_file() {
        let raw = std::fs::read_to_string(&deno_path)
            .map_err(|e| MgError::Other(format!("read deno.lock: {e}")))?;
        let read = read_deno_lock(&raw)?;
        combined.npm_pins.extend(read.npm_pins);
        combined.jsr_skipped.extend(read.jsr_skipped);
    }
    Ok(combined)
}
