//! Integration tests for PhaseProfiler — timing, report formatting, and JSON output.

use std::collections::HashMap;
use std::time::{Duration, Instant};

struct PhaseProfiler {
    phases: HashMap<String, (Instant, Option<Duration>)>,
    order: Vec<String>,
}

impl PhaseProfiler {
    fn new() -> Self {
        Self {
            phases: HashMap::new(),
            order: Vec::new(),
        }
    }

    fn start(&mut self, phase: &str) {
        let key = phase.to_string();
        if !self.phases.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.phases.insert(key, (Instant::now(), None));
    }

    fn end(&mut self, phase: &str) {
        let key = phase.to_string();
        if let Some((start, _)) = self.phases.get(&key) {
            let elapsed = start.elapsed();
            self.phases.insert(key, (*start, Some(elapsed)));
        }
    }

    fn report(&self) -> String {
        let mut total = Duration::default();
        let mut lines = Vec::new();

        for phase in &self.order {
            if let Some((_, Some(duration))) = self.phases.get(phase) {
                total += *duration;
                lines.push(format!(
                    "║ {:<14}│ {:>9} ║",
                    phase,
                    format_duration(*duration)
                ));
            }
        }

        let sep = "╠═══════════════╪═══════════╣";

        format!(
            "╔═══════════════╤═══════════╗\n\
             {}\n\
             {}\n\
             ║ {:<14}│ {:>9} ║\n\
             ╚═══════════════╧═══════════╝",
            lines.join("\n"),
            sep,
            "TOTAL",
            format_duration(total),
        )
    }

    fn report_json(&self) -> String {
        let mut phases = Vec::new();
        let mut total_ms = 0u64;

        for phase in &self.order {
            if let Some((_, Some(duration))) = self.phases.get(phase) {
                let ms = duration.as_millis() as u64;
                total_ms += ms;
                phases.push(serde_json::json!({
                    "phase": phase,
                    "duration_ms": ms,
                    "duration_secs": duration.as_secs_f64(),
                }));
            }
        }

        serde_json::to_string_pretty(&serde_json::json!({
            "phases": phases,
            "total_ms": total_ms,
            "total_secs": total_ms as f64 / 1000.0,
        }))
        .unwrap_or_default()
    }
}

impl Default for PhaseProfiler {
    fn default() -> Self {
        Self::new()
    }
}

fn format_duration(d: Duration) -> String {
    format!("{:.3}s", d.as_secs_f64())
}

#[test]
fn test_profiler_start_end_single_phase() {
    let mut profiler = PhaseProfiler::new();
    profiler.start("resolve");
    std::thread::sleep(Duration::from_millis(10));
    profiler.end("resolve");

    let report = profiler.report();
    assert!(report.contains("resolve"));
    assert!(report.contains("TOTAL"));
    assert!(report.starts_with("╔═══════════════╤═══════════╗"));
    assert!(report.ends_with("╚═══════════════╧═══════════╝"));
}

#[test]
fn test_profiler_multiple_phases() {
    let mut profiler = PhaseProfiler::new();

    profiler.start("resolve");
    std::thread::sleep(Duration::from_millis(5));
    profiler.end("resolve");

    profiler.start("fetch");
    std::thread::sleep(Duration::from_millis(5));
    profiler.end("fetch");

    profiler.start("install");
    std::thread::sleep(Duration::from_millis(5));
    profiler.end("install");

    let report = profiler.report();
    assert!(report.contains("resolve"));
    assert!(report.contains("fetch"));
    assert!(report.contains("install"));
    assert!(report.contains("TOTAL"));

    let lines: Vec<&str> = report.lines().collect();
    let resolve_idx = lines.iter().position(|l| l.contains("resolve")).unwrap();
    let fetch_idx = lines.iter().position(|l| l.contains("fetch")).unwrap();
    let install_idx = lines.iter().position(|l| l.contains("install")).unwrap();

    assert!(resolve_idx < fetch_idx, "resolve should come before fetch");
    assert!(fetch_idx < install_idx, "fetch should come before install");
}

#[test]
fn test_profiler_report_no_phases() {
    let profiler = PhaseProfiler::new();
    let report = profiler.report();
    assert!(report.contains("TOTAL"));
    assert!(report.contains("0.000s"));
}

#[test]
fn test_profiler_restart_phase() {
    let mut profiler = PhaseProfiler::new();

    profiler.start("fetch");
    std::thread::sleep(Duration::from_millis(10));
    profiler.end("fetch");

    let first_duration = profiler.phases.get("fetch").unwrap().1.unwrap();

    profiler.start("fetch");
    std::thread::sleep(Duration::from_millis(5));
    profiler.end("fetch");

    let second_duration = profiler.phases.get("fetch").unwrap().1.unwrap();

    assert!(
        second_duration < first_duration,
        "Second duration should be shorter"
    );
}

#[test]
fn test_profiler_report_json_valid_json() {
    let mut profiler = PhaseProfiler::new();

    profiler.start("resolve");
    std::thread::sleep(Duration::from_millis(5));
    profiler.end("resolve");

    profiler.start("fetch");
    std::thread::sleep(Duration::from_millis(5));
    profiler.end("fetch");

    let json_str = profiler.report_json();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert!(parsed.get("phases").is_some());
    assert!(parsed.get("total_ms").is_some());
    assert!(parsed.get("total_secs").is_some());

    let phases = parsed["phases"].as_array().unwrap();
    assert_eq!(phases.len(), 2);

    assert_eq!(phases[0]["phase"], "resolve");
    assert!(phases[0]["duration_ms"].as_u64().unwrap_or(0) > 0);

    assert_eq!(phases[1]["phase"], "fetch");
    assert!(phases[1]["duration_ms"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn test_profiler_report_json_empty() {
    let profiler = PhaseProfiler::new();
    let json_str = profiler.report_json();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let phases = parsed["phases"].as_array().unwrap();
    assert!(phases.is_empty());
    assert_eq!(parsed["total_ms"].as_u64().unwrap(), 0);
    assert_eq!(parsed["total_secs"].as_f64().unwrap(), 0.0);
}

#[test]
fn test_profiler_timing_accuracy_within_bounds() {
    let mut profiler = PhaseProfiler::new();

    profiler.start("slow-phase");
    std::thread::sleep(Duration::from_millis(50));
    profiler.end("slow-phase");

    let duration = profiler.phases.get("slow-phase").unwrap().1.unwrap();
    let ms = duration.as_millis();

    assert!(ms >= 40, "Expected at least 40ms, got {}ms", ms);
    assert!(ms <= 500, "Expected at most 500ms, got {}ms", ms);
}

#[test]
fn test_profiler_timing_multiple_short_phases() {
    let mut profiler = PhaseProfiler::new();
    let mut total_measured = Duration::default();

    for i in 0..5 {
        let name = format!("phase-{}", i);
        profiler.start(&name);
        std::thread::sleep(Duration::from_millis(2));
        profiler.end(&name);
        total_measured += profiler.phases.get(&name).unwrap().1.unwrap();
    }

    let report = profiler.report();
    let json_str = profiler.report_json();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert!(parsed["total_ms"].as_u64().unwrap() > 0);

    for i in 0..5 {
        let name = format!("phase-{}", i);
        assert!(report.contains(&name));
        assert!(parsed["phases"][i]["phase"] == name);
    }
}

#[test]
fn test_profiler_end_before_start_is_noop() {
    let mut profiler = PhaseProfiler::new();

    profiler.end("never-started");
    let report = profiler.report();

    assert!(!report.contains("never-started"));
}

#[test]
fn test_profiler_report_json_fields() {
    let mut profiler = PhaseProfiler::new();

    profiler.start("build");
    std::thread::sleep(Duration::from_millis(10));
    profiler.end("build");

    let json_str = profiler.report_json();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let phase = &parsed["phases"][0];
    assert_eq!(phase["phase"], "build");
    assert!(phase["duration_ms"].is_number());
    assert!(phase["duration_secs"].is_number());
    assert!(
        (phase["duration_secs"].as_f64().unwrap()
            - phase["duration_ms"].as_f64().unwrap() / 1000.0)
            .abs()
            < 0.001
    );
}

#[test]
fn test_profiler_total_matches_sum_of_phases() {
    let mut profiler = PhaseProfiler::new();

    profiler.start("a");
    std::thread::sleep(Duration::from_millis(3));
    profiler.end("a");

    profiler.start("b");
    std::thread::sleep(Duration::from_millis(3));
    profiler.end("b");

    let json_str = profiler.report_json();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let phase_sum: u64 = parsed["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["duration_ms"].as_u64().unwrap_or(0))
        .sum();

    assert_eq!(parsed["total_ms"].as_u64().unwrap(), phase_sum);
}

#[test]
fn test_profiler_new_is_default() {
    let p1 = PhaseProfiler::new();
    let p2 = PhaseProfiler::default();
    assert_eq!(p1.order.len(), p2.order.len());
    assert_eq!(p1.phases.len(), p2.phases.len());
}
