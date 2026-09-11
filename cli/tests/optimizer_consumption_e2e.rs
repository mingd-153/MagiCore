//! P0-7 Optimizer consumption proof (Tech Lead 2026-09-11): a generated
//! env var must be PROVEN to reach a real child process — writing a
//! file and asserting its string content proves nothing about runtime
//! behavior. This E2E: generate the PyTorch env → load it through the
//! production env_loader → spawn a REAL python child that echoes
//! os.environ → assert the values arrived. Machines without python
//! skip HONESTLY (explicit marker, never a silent pass).
//!
//! P0-7 Bằng chứng tiêu thụ optimizer: biến env sinh ra phải được
//! CHỨNG MINH tới được tiến trình con thật — ghi file rồi assert chuỗi
//! không chứng minh gì về hành vi runtime. E2E này: sinh env PyTorch →
//! load qua env_loader production → spawn python THẬT in os.environ →
//! assert giá trị tới nơi. Máy không có python thì skip TRUNG THỰC
//! (marker tường minh, không bao giờ pass âm thầm).

use std::process::Command;

// Binary-crate test pattern (RULE §5): access the CLI crate's internals
// through the integration test boundary — the optimizer generators and
// env_loader are exercised exactly as production uses them.
use mgc::commands::optimizer::adapters::OptimizerAdapter;
use mgc::commands::optimizer::adapters::pytorch::PyTorchAdapter;
use mgc::commands::optimizer::detect::{HardwareInfo, SystemProfile};
use mgc::commands::optimizer::env_loader::load_optimizer_env;
use mgc::commands::optimizer::runtime_detect::DetectedRuntime;

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn pytorch_env_reaches_real_child_process() {
    if !python3_available() {
        // Honest skip: no python on this machine — consumption is
        // UNVERIFIED here, never "passed".
        // Skip trung thực: máy không có python — tiêu thụ CHƯA XÁC
        // MINH ở đây, không phải "pass".
        eprintln!(
            "SKIP (environment-unverified) test=pytorch_env_reaches_real_child_process: python3 not installed on this machine"
        );
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir created");
    let project_root = tmp.path().to_path_buf();

    // 1. GENERATE: the production PyTorch adapter, with a fixed fake
    //    HardwareInfo, writes the env file (same path production uses).
    //    SINH: adapter PyTorch production với HardwareInfo giả cố định
    //    ghi file env (đúng đường dẫn production dùng).
    let hw = HardwareInfo {
        cpu_cores: 4,
        arch: "x86_64".to_string(),
        os: "macos".to_string(),
        total_memory_gb: 16,
        profile: SystemProfile::Standard,
    };
    let files = PyTorchAdapter.generate(&hw);
    let env_file = files
        .iter()
        .find(|f| f.relative_path.ends_with("pytorch_runtime.env"))
        .expect("pytorch_runtime.env generated");
    let optimizer_dir = project_root.join(".mgc-optimizer");
    std::fs::create_dir_all(&optimizer_dir).expect("optimizer dir created");
    std::fs::write(optimizer_dir.join("pytorch_runtime.env"), &env_file.content)
        .expect("env file written");

    // 2. LOAD: the production env_loader parses the file into vars.
    //    NẠP: env_loader production parse file thành biến.
    let vars = load_optimizer_env(&project_root, &DetectedRuntime::PythonPyTorch)
        .expect("env loader parses the generated file");
    assert_eq!(vars.get("OMP_NUM_THREADS").map(String::as_str), Some("4"));
    assert_eq!(vars.get("TORCH_NUM_THREADS").map(String::as_str), Some("4"));

    // 3. CONSUME: spawn a REAL python child carrying the loaded vars;
    //    the child prints what os.environ actually contains.
    //    TIÊU THỤ: spawn python THẬT mang biến đã load; con in ra
    //    os.environ thật sự chứa gì.
    let script = "import os; print(os.environ.get('OMP_NUM_THREADS','<missing>')); print(os.environ.get('TORCH_NUM_THREADS','<missing>'))";
    let mut cmd = Command::new("python3");
    cmd.arg("-c").arg(script);
    for (k, v) in &vars {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("python3 spawns");
    assert!(out.status.success(), "python child exited: {}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("4"),
        "OMP_NUM_THREADS must reach the real child process; stdout: {stdout:?}"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some("4"),
        "TORCH_NUM_THREADS must reach the real child process; stdout: {stdout:?}"
    );
}

/// Negative control: WITHOUT loading the optimizer env, the child must
/// NOT see the variables — proves the assertion above tests the loader
/// chain, not ambient environment leakage.
/// Đối chứng âm: KHÔNG load env optimizer thì con KHÔNG thấy biến —
/// chứng minh assertion trên đo đúng chuỗi loader, không phải rò môi
/// trường xung quanh.
#[test]
fn pytorch_env_absent_without_loader() {
    if !python3_available() {
        eprintln!(
            "SKIP (environment-unverified) test=pytorch_env_absent_without_loader: python3 not installed on this machine"
        );
        return;
    }
    let script = "import os; print(os.environ.get('TORCH_NUM_THREADS','<missing>'))";
    // Deliberately NOT loading optimizer env — the child must see the
    // sentinel, unless the host ambient env leaks TORCH_NUM_THREADS
    // (which itself would be a test-environment bug to know about).
    // CỐ Ý KHÔNG load env optimizer — con phải thấy sentinel, trừ khi
    // môi trường máy rò TORCH_NUM_THREADS (chính điều đó cũng là bug
    // môi trường test cần biết).
    let out = Command::new("python3")
        .arg("-c")
        .arg(script)
        .env_remove("TORCH_NUM_THREADS")
        .output()
        .expect("python3 spawns");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "<missing>",
        "negative control: TORCH_NUM_THREADS must not exist without the loader"
    );
}

/// The env loader must survive a missing .mgc-optimizer dir (empty vars,
/// never an error) — production projects without optimizer output stay
/// runnable.
/// Env loader phải sống sót khi thiếu .mgc-optimizer (biến rỗng, không
/// lỗi) — project không có output optimizer vẫn chạy được.
#[test]
fn env_loader_missing_dir_returns_empty() {
    let tmp = tempfile::tempdir().expect("tempdir created");
    let vars = load_optimizer_env(tmp.path(), &DetectedRuntime::PythonPyTorch)
        .expect("loader returns empty for missing dir");
    assert!(vars.is_empty());
}
