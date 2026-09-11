//! JSONC strip contract tests (RULE §5 — per-crate tests in `tests/`).
//! One shared implementation for mgc-audit (scanner) + mgc-lockfile
//! (importer): these tests pin the QUOTE-AWARE comment cut and the
//! TRAILING-comma lookahead so neither consumer can regress silently.
//! Hợp đồng test cho strip_jsonc: ghim cắt comment nhận-biết-quote và
//! lookahead dấu phẩy cuối để không consumer nào hồi quy âm thầm.

use mgc_types::strip_jsonc;

#[test]
fn line_comments_outside_strings_are_stripped() {
    let raw = "{\n  // registry comment\n  \"name\": \"left\",\n}\n";
    let out = strip_jsonc(raw);
    assert!(
        !out.contains("registry comment"),
        "comment must be cut: {out}"
    );
    assert!(
        out.contains("\"left\""),
        "value after comment survives: {out}"
    );
}

#[test]
fn slash_slash_inside_string_is_not_a_comment() {
    // A tarball URL contains "//" — the quote state must protect it.
    // URL tarball chứa "//" — trạng thái quote phải bảo vệ nó.
    let raw = "{\n  \"resolved\": \"https://registry.npmjs.org/pkg/-/pkg-1.0.0.tgz\",\n}\n";
    let out = strip_jsonc(raw);
    assert!(
        out.contains("https://registry.npmjs.org/pkg/-/pkg-1.0.0.tgz"),
        "URL inside quotes must survive: {out}"
    );
}

#[test]
fn trailing_comma_before_close_bracket_is_stripped() {
    // Bun's writer pretty-prints: the trailing comma lands at END of
    // its own line and the close bracket on the NEXT line — that is the
    // contract the lookahead enforces. (A same-line `",]"` shape is not
    // what bun emits and is out of contract.)
    // Writer bun ghi pretty-print: dấu phẩy cuối nằm CUỐI dòng của nó,
    // dấu đóng ở dòng KẾ — đó là hợp đồng lookahead thực thi (shape
    // `",]"` cùng dòng không phải bun ghi, ngoài hợp đồng).
    let raw = "{\n  \"a\": 1,\n  \"b\": [\n    \"x\",\n    \"y\",\n  ]\n}\n";
    let out = strip_jsonc(raw);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("stripped output must be valid JSON ({e}): {out}"));
    assert_eq!(parsed["b"].as_array().map(|a| a.len()), Some(2));
}

#[test]
fn comma_before_next_member_is_kept() {
    // A comma whose next line opens another member is LEGAL JSON — the
    // lookahead must keep it (over-stripping corrupts valid lockfiles).
    // Dấu phẩy mà dòng kế mở member khác là JSON hợp pháp — lookahead
    // phải giữ nó (cắt quá sẽ làm gãy lockfile hợp lệ).
    let raw = "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}\n";
    let out = strip_jsonc(raw);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("legal commas must survive ({e}): {out}"));
    assert_eq!(parsed["c"].as_i64(), Some(3));
}

#[test]
fn bun_writer_shape_roundtrip() {
    // The real bun writer emits JSONC: trailing commas + comments. This
    // is the exact fixture shape the shared strip must keep accepting.
    // Writer bun thật ghi JSONC: dấu phẩy cuối + comment — đúng shape
    // fixture mà bản strip chung phải tiếp tục chấp nhận.
    let raw = "{\n  // this is a comment\n  \"packages\": {\n    \"left-pad\": [\n      \"left-pad@4.0.0\",\n      \"https://registry.npmjs.org/left-pad/-/left-pad-4.0.0.tgz\",\n      \"sha512-abc123\",\n    ],\n  },\n}\n";
    let out = strip_jsonc(raw);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("bun JSONC shape must parse after strip ({e}): {out}"));
    let entry = &parsed["packages"]["left-pad"];
    assert_eq!(entry[0].as_str(), Some("left-pad@4.0.0"));
    assert!(matches!(entry[1].as_str(), Some(url) if url.starts_with("https://")));
    assert!(matches!(entry[2].as_str(), Some(hash) if hash.starts_with("sha512-")));
}

#[test]
fn empty_and_blank_input_yield_no_content() {
    // Zero lines in → zero content out (an empty lockfile body is not
    // corrupt, it is empty); blank lines are preserved as-is.
    // Không dòng vào → không nội dung ra (lockfile rỗng không phải
    // hỏng); dòng trống giữ nguyên.
    assert_eq!(strip_jsonc(""), "");
    assert_eq!(strip_jsonc("   \n\n"), "   \n\n");
}
