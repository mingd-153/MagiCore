use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct PhaseProfiler {
    phases: HashMap<String, (Instant, Option<Duration>)>,
    order: Vec<String>,
}

impl PhaseProfiler {
    pub fn new() -> Self {
        Self {
            phases: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn start(&mut self, phase: &str) {
        let key = phase.to_string();
        if !self.phases.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.phases.insert(key, (Instant::now(), None));
    }

    pub fn end(&mut self, phase: &str) {
        let key = phase.to_string();
        if let Some((start, _)) = self.phases.get(&key) {
            let elapsed = start.elapsed();
            self.phases.insert(key, (*start, Some(elapsed)));
        }
    }

    pub fn report(&self) -> String {
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

    pub fn report_json(&self) -> String {
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
