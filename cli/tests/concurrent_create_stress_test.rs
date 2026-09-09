//! Concurrent create stress (§3.1 process/concurrency stress) — binary-real
//! E2E: nhiều `mgc create-*` chạy song song KHÔNG được ghi chéo file giữa
//! các project (kế hoạch: "Nhiều mgc create-* với project name khác nhau —
//! Không ghi chéo file/template/cache").
//!
//! Plus: same-name race — only one process may win; the loser must fail
//! cleanly and never leave a partial/corrupted project (atomic staging).

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

fn find_mgc_binary() -> String {
    // Cargo-provided binary path: fresh per test invocation, portable across
    // platforms (no stale target/ lookup, .exe suffix handled by cargo).
    // Binary do cargo cung cấp: tươi theo lần chạy test, portable mọi nền
    // tảng (không dò target/ cũ, cargo tự lo đuôi .exe).
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent()?
                    .parent()
                    .map(|parent| parent.join("mgc"))
                    .map(|p| p.to_string_lossy().to_string())
            })
        })
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

fn spawn_create(
    mgc: &str,
    cwd: &std::path::Path,
    framework: &str,
    name: &str,
) -> std::process::Child {
    Command::new(mgc)
        .arg("create-web")
        .arg(framework)
        .arg(name)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn mgc create")
}

#[test]
fn test_concurrent_create_distinct_names_no_cross_writes() {
    // 8 parallel create-web processes with distinct project names in one
    // shared cwd — every project must materialize independently with its
    // own files; no cross-project leakage of name/identifier into another.
    // 8 process create-web song song, tên khác nhau, cùng cwd — mỗi project
    // phải nguyên vẹn với đúng tên của nó, không ghi chéo.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    const N: usize = 8;

    let mut children: Vec<(std::process::Child, String)> = Vec::new();
    for i in 0..N {
        let name = format!("parallel-proj-{i}");
        let child = spawn_create(&mgc, sandbox.path(), "vanilla", &name);
        children.push((child, name));
    }

    let mut failures = Vec::new();
    for (child, name) in children {
        let output = child.wait_with_output().expect("wait failed");
        if !output.status.success() {
            failures.push(format!(
                "{name}: exit={:?} stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "all parallel creates must succeed: {failures:?}"
    );

    // Each project must exist and contain its own name in mgc.toml — cross
    // writes would put the wrong project name into another project.
    // Mỗi project phải chứa đúng tên của chính nó — ghi chéo sẽ làm lẫn tên.
    for i in 0..N {
        let dir = sandbox.path().join(format!("parallel-proj-{i}"));
        assert!(
            dir.join("mgc.toml").exists(),
            "project {i} missing mgc.toml"
        );
        let toml = std::fs::read_to_string(dir.join("mgc.toml")).unwrap();
        assert!(
            toml.contains(&format!("parallel-proj-{i}")),
            "project {i} mgc.toml must carry its own name"
        );
        for j in 0..N {
            if j != i {
                assert!(
                    !toml.contains(&format!("parallel-proj-{j}")),
                    "project {i} contains foreign name from project {j} — cross-write!"
                );
            }
        }
    }
}

#[test]
fn test_concurrent_create_same_name_single_winner() {
    // 4 parallel processes race on the SAME project name — exactly one may
    // win; the rest must fail with dir-already-exists and the winner must
    // be complete, not a merge of multiple writers.
    // 4 process cùng tên — đúng 1 thắng, kẻ thua fail rõ ràng, project thắng
    // phải nguyên vẹn không phải merge của nhiều writer.
    let mgc = find_mgc_binary();
    let sandbox = TempDir::new().unwrap();
    const N: usize = 4;
    let name = "race-single-winner";

    let mut children = Vec::new();
    for _ in 0..N {
        children.push(spawn_create(&mgc, sandbox.path(), "vanilla", name));
    }

    let mut success = 0;
    let mut failed = 0;
    for child in children {
        let output = child.wait_with_output().expect("wait failed");
        if output.status.success() {
            success += 1;
        } else {
            failed += 1;
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            // Losers must fail with a clear reason, never panic gibberish.
            // Either the final dir exists, or the atomic claim slot is held.
            // Kẻ thua phải fail có lý do rõ: hoặc dir đã tồn tại, hoặc
            // claim-slot nguyên tử đang bị process khác giữ.
            assert!(
                stderr.contains("already exists") || stderr.contains("in progress"),
                "loser must explain failure, got: {stderr}"
            );
        }
    }
    assert_eq!(success, 1, "exactly one winner expected, got {success}");
    assert_eq!(failed, N - 1, "the rest must fail cleanly");

    // The single winner's project must be structurally complete — atomic
    // staging guarantees no interleaved partial content. vanilla projects
    // materialize mgc.toml + index.html (README lives in layer-specific
    // files), so assert against the real contract.
    // Project thắng phải hoàn chỉnh — staging nguyên tử đảm bảo không trộn.
    // Project vanilla tạo mgc.toml + index.html (README nằm ở file theo
    // layer), assert đúng hợp đồng thật.
    let dir = sandbox.path().join(name);
    assert!(dir.join("mgc.toml").exists(), "winner project must exist");
    assert!(
        dir.join("index.html").exists(),
        "winner entrypoint must exist"
    );
    let toml = std::fs::read_to_string(dir.join("mgc.toml")).unwrap();
    assert!(toml.contains(name), "winner must carry its own name");
    // No staging leftovers AND no leaked claim-slot files in the shared cwd.
    // Không staging rác và không claim-slot .lock sót lại trong cwd chung.
    let entries: Vec<String> = std::fs::read_dir(sandbox.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != name && n != ".DS_Store")
        .collect();
    assert!(
        entries.is_empty(),
        "no staging/claim leaks in shared cwd, found: {entries:?}"
    );
    assert!(
        !sandbox
            .path()
            .join(format!(".mgc-create-{name}.lock"))
            .exists(),
        "claim slot must be released after success"
    );
}
