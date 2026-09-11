//! `jsonc.rs` — Shared JSONC cleanup (single source of truth).
//!
//! Bun's `bun.lock` writer emits JSONC: `//` line comments and TRAILING
//! commas after the final array element / object member — plain
//! serde_json rejects both. Both mgc-audit (lockfile advisory scanner)
//! and mgc-lockfile (migration importer) must accept what real `bun
//! install` writes, so the cleanup lives HERE — one implementation,
//! two consumers (P1 dedup, Tech Lead 2026-09-11).
//!
//! Dọn JSONC dùng chung: writer của bun ghi comment dòng `//` và dấu
//! phẩy CUỐI — serde_json thuần từ chối cả hai. mgc-audit (scanner
//! advisory) và mgc-lockfile (importer migration) cùng cần chấp nhận
//! đúng cái `bun install` ghi ra nên hàm nằm Ở ĐÂY — một bản implement,
//! hai nơi dùng (dedup P1).

/// Strip JSONC: `//` line comments (quote-aware) and TRAILING commas
/// (lookahead). A `//` inside a string ("https://...") is NOT a comment
/// — quote state is tracked so tarball URLs survive. A comma at end of
/// line is only trailing when the FOLLOWING non-empty line starts with
/// a close bracket; stripping without the lookahead would corrupt valid
/// JSON (a comma before the next member is legal).
///
/// Cắt JSONC: comment dòng `//` (nhận-biết-quote) và dấu phẩy CUỐI
/// (lookahead). `//` trong chuỗi ("https://...") KHÔNG phải comment —
/// theo dõi quote để URL tarball sống sót. Dấu phẩy cuối dòng chỉ là
/// "cuối" khi dòng khác-rỗng kế bắt đầu bằng dấu đóng; thiếu lookahead
/// sẽ làm gãy JSON hợp lệ (dấu phẩy trước member kế tiếp là hợp pháp).
pub fn strip_jsonc(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut out = String::with_capacity(raw.len());
    for (i, line) in lines.iter().enumerate() {
        // Quote-aware comment cut: find `//` only OUTSIDE strings.
        // Cắt comment nhận-biết-quote: chỉ tìm `//` NGOÀI chuỗi.
        let mut cleaned = *line;
        let mut in_quote = false;
        let mut quote_end = line.len();
        for (idx, ch) in line.char_indices() {
            if ch == '"' {
                in_quote = !in_quote;
            } else if ch == '/' && !in_quote && line[idx..].starts_with("//") {
                quote_end = idx;
                break;
            }
        }
        if quote_end < line.len() {
            cleaned = &line[..quote_end];
        }
        let trimmed = cleaned.trim_end();
        if trimmed.ends_with(',') {
            // Lookahead: the comma is TRAILING only when the next
            // non-empty line begins with a close bracket.
            // Nhìn trước: dấu phẩy chỉ CUỐI khi dòng khác-rỗng kế tiếp
            // bắt đầu bằng dấu đóng.
            let next_nonempty = lines[i + 1..].iter().find(|l| !l.trim().is_empty());
            let closes_next = next_nonempty
                .map(|l| {
                    let t = l.trim_start();
                    t.starts_with('}') || t.starts_with(']')
                })
                .unwrap_or(false);
            if closes_next && let Some(no_comma) = trimmed.strip_suffix(',') {
                cleaned = no_comma;
            }
        }
        out.push_str(cleaned);
        out.push('\n');
    }
    out
}
