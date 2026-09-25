//! Framework ownership registry tests live outside `src/` by project rule.
//! (Test registry framework nằm ngoài `src/` theo quy định dự án.)

use super::*;
use std::collections::HashMap;

const WIZARD_CORES: &[(&str, &str)] = &[
    ("ai.rs", "ai"),
    ("app.rs", "app"),
    ("cicd.rs", "cicd"),
    ("cloud.rs", "clo"),
    ("game.rs", "game"),
    ("hardware.rs", "hardware"),
    ("iot.rs", "iot"),
    ("lib.rs", "lib"),
    ("web.rs", "web"),
];

fn wizard_ids(source: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("Answer::new(") {
        rest = &rest[start + "Answer::new(".len()..];
        let Some(first) = rest.find('"') else { break };
        let after_first = &rest[first + 1..];
        let Some(first_end) = after_first.find('"') else {
            break;
        };
        let after_display = &after_first[first_end + 1..];
        let Some(second) = after_display.find('"') else {
            break;
        };
        let after_second = &after_display[second + 1..];
        let Some(second_end) = after_second.find('"') else {
            break;
        };
        ids.push(after_second[..second_end].to_string());
        rest = &after_second[second_end + 1..];
    }
    ids
}

#[test]
fn every_wizard_answer_has_exactly_one_record() {
    let wizard_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/wizard");
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    for record in RECORDS {
        *counts.entry((record.core, record.framework)).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    for (file, core) in WIZARD_CORES {
        let source = std::fs::read_to_string(wizard_dir.join(file))
            .unwrap_or_else(|_| panic!("wizard source missing: {file}"));
        for id in wizard_ids(&source) {
            match counts.get(&(core, id.as_str())) {
                Some(1) => {}
                Some(n) => panic!("duplicate framework record for {core}/{id}: {n}"),
                None => missing.push(format!("{core}/{id}")),
            }
        }
    }
    assert!(
        missing.is_empty(),
        "wizard ids without records: {missing:?}"
    );
}

#[test]
fn framework_route_is_not_misreported_as_dependency_support() {
    let expected_engine_routes = [
        ("web", "vanilla"),
        ("web", "ts"),
        ("web", "node"),
        ("lib", "ts"),
        ("lib", "rust"),
        ("lib", "python"),
        ("clo", "cdk"),
        ("clo", "pulumi"),
    ];
    for record in RECORDS {
        if record.status == FrameworkStatus::NativeEngine {
            assert!(expected_engine_routes.contains(&(record.core, record.framework)));
            assert_eq!(status_string(&record.status), "mgc-engine-path");
        } else {
            assert_eq!(status_string(&record.status), "scaffold-only");
        }
    }
}

#[test]
fn operation_ownership_json_is_complete_and_derived_from_gate() {
    let valid_owners = ["mgc-native", "scaffold-only", "unsupported"];
    for core in [
        "ai", "app", "cicd", "clo", "game", "hardware", "iot", "lib", "web",
    ] {
        let rows = qualification_json(core);
        let rows = rows.as_array().expect("core records array");
        for row in rows {
            let operations = row["dependency_ownership"].as_object().expect("operations");
            assert_eq!(
                operations.len(),
                crate::commands::dep_gate::DepOp::ALL.len()
            );
            for op in crate::commands::dep_gate::DepOp::ALL {
                let cell = &operations[op.as_str()];
                let owner = cell["owner"].as_str().expect("owner string");
                assert!(valid_owners.contains(&owner), "unexpected owner {owner}");
                assert_ne!(owner, "delegated", "delegation is not native ownership");
            }
        }
    }
}

#[test]
fn cloud_native_engine_label_is_conditional_and_terraform_is_not_native() {
    for framework in ["cdk", "pulumi"] {
        let row = qualification_json("clo")
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["framework"] == framework)
            .cloned()
            .expect("cloud framework record");
        assert_eq!(row["status"], "mgc-engine-path");
        for op in crate::commands::dep_gate::DepOp::ALL {
            let cell = &row["dependency_ownership"][op.as_str()];
            if cell["owner"] == "mgc-native" {
                assert_eq!(
                    cell["requires"],
                    "package.json; embedded MGC JavaScript engine"
                );
            }
        }
    }
    let cloud_rows = qualification_json("clo");
    let terraform = cloud_rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["framework"] == "terraform")
        .unwrap();
    assert!(
        terraform["dependency_ownership"]
            .as_object()
            .unwrap()
            .values()
            .all(|cell| cell["owner"] != "mgc-native")
    );
}
