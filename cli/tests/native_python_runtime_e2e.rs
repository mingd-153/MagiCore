//! Verify an MGC-managed Python lock is visible to an actual Python child
//! spawned through `mgc run`, without pip/uv or ambient site-packages.

#![allow(clippy::unwrap_used)]

use base64::Engine;
use mgc_lockfile::{EcosystemTag, Lockfile, Package};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .expect("CARGO_BIN_EXE_mgc unavailable — run through cargo test")
}

fn runtime_probe_wheel() -> Vec<u8> {
    let module = b"VALUE = 'loaded-from-ai-owner'\n";
    let metadata = b"Name: mgc-runtime-probe\nVersion: 2.0.0\n";
    let dist_info = "mgc_runtime_probe-2.0.0.dist-info";
    let metadata_path = format!("{dist_info}/METADATA");
    let record_path = format!("{dist_info}/RECORD");
    let files = [
        ("mgc_runtime_probe.py", module.as_slice()),
        (metadata_path.as_str(), metadata.as_slice()),
    ];
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut record = String::new();
    for (path, bytes) in files {
        record.push_str(&format!(
            "{path},sha256={},{}\n",
            b64.encode(Sha256::digest(bytes)),
            bytes.len()
        ));
    }
    record.push_str(&format!("{record_path},,\n"));
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, bytes) in files {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.start_file(record_path, options).unwrap();
    writer.write_all(record.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn mgc_run_exposes_only_verified_mgc_python_materialization_to_python() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let wheels_root = home.path().join(".magicore/store/pypi/wheels");
    std::fs::create_dir_all(&wheels_root).unwrap();
    let wheel_name = "mgc_runtime_probe-2.0.0-py3-none-any.whl";
    let wheel_url = format!("https://files.pythonhosted.org/packages/{wheel_name}");
    let wheel = runtime_probe_wheel();
    let digest = mgc_resolver::protocols::sha256_hex(&wheel);
    let entry = mgc_resolver::protocols::ResolvedEntry {
        name: "mgc-runtime-probe".to_string(),
        version: "2.0.0".to_string(),
        deps: Vec::new(),
        artifact_url: wheel_url.clone(),
        sha256: digest.clone(),
        extra_markers: Vec::new(),
    };
    let protocol = mgc_resolver::protocols::PypiProtocol::new("https://pypi.org");
    protocol.materialize(&entry, &wheel, &wheels_root).unwrap();
    let site_path = protocol
        .materialize_importable(&entry, &wheel, &wheels_root)
        .unwrap()
        .unwrap();

    let python = if cfg!(windows) { "python" } else { "python3" };
    std::fs::write(
        project.path().join("mgc.toml"),
        format!("name = \"python-runtime-probe\"\necosystem = \"ai\"\n[ai]\nframework = \"python-agent\"\n[scripts]\ncheck = \"{python} check.py\"\n"),
    )
    .unwrap();
    std::fs::write(
        project.path().join("pyproject.toml"),
        "[project]\nname = \"python-runtime-probe\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = []\n",
    )
    .unwrap();
    mgc_config::project::ProjectConfig::write_core_marker_at(project.path(), "ai").unwrap();
    std::fs::write(
        project.path().join("check.py"),
        "import mgc_runtime_probe\nassert mgc_runtime_probe.VALUE == 'loaded-from-ai-owner'\nprint('MGC_RUNTIME_PROBE:run')\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/agent.py"),
        "import mgc_runtime_probe\nassert mgc_runtime_probe.VALUE == 'loaded-from-ai-owner'\nprint('MGC_RUNTIME_PROBE:dev', mgc_runtime_probe.VALUE)\n",
    )
    .unwrap();
    let mut lock = Lockfile::new();
    let mut lib_package = Package::new(
        "mgc-runtime-probe".into(),
        "1.0.0".into(),
        "https://pypi.org/project/mgc-runtime-probe/".into(),
        "sha256-lib-integrity".into(),
    );
    lib_package.ecosystem = EcosystemTag::Python;
    lib_package.owner_core = Some("lib".into());
    lock.add_package(lib_package);
    let mut ai_package = Package::new(
        "mgc-runtime-probe".into(),
        "2.0.0".into(),
        wheel_url,
        format!("sha256-{digest}"),
    );
    ai_package.ecosystem = EcosystemTag::Python;
    ai_package.owner_core = Some("ai".into());
    lock.add_package(ai_package);
    std::fs::write(
        project.path().join("mgc.lock"),
        mgc_lockfile::writer::serialize_lockfile(&lock).unwrap(),
    )
    .unwrap();

    for (args, expected_output) in [
        (vec!["run", "check"], "MGC_RUNTIME_PROBE:run"),
        (vec!["dev", "--core", "ai"], "MGC_RUNTIME_PROBE:dev"),
    ] {
        let mut command = Command::new(mgc_binary());
        command
            .args(&args)
            .current_dir(project.path())
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env_remove("PYTHONPATH");
        let output = command.output().expect("spawn mgc");
        assert!(
            output.status.success(),
            "MGC Python child must see the lock-verified MGC site path for {args:?}; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(expected_output),
            "the Python child for {args:?} must execute and print its probe marker; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        !site_path.join("__pycache__").exists(),
        "MGC Python runtime must not write bytecode into the shared materialization"
    );
}
