use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_FILE: &str = "conversation_log.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Phase {
    pub id: u64,
    pub description: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Log {
    pub phases: Vec<Phase>,
}

fn log_path() -> PathBuf {
    // Store log file alongside the crate directory (project root)
    let mut path = std::env::current_dir().expect("cannot get cwd");
    path.push(LOG_FILE);
    path
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load_log() -> Result<Log> {
    let path = log_path();
    if path.exists() {
        let mut data = String::new();
        fs::File::open(&path)?.read_to_string(&mut data)?;
        let log: Log = serde_json::from_str(&data)?;
        Ok(log)
    } else {
        Ok(Log::default())
    }
}

fn save_log(log: &Log) -> Result<()> {
    let path = log_path();
    let json = serde_json::to_string_pretty(log)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

/// Start a new phase and persist it. Returns the phase id.
pub fn start_phase(description: &str) -> Result<u64> {
    let mut log = load_log()?;
    let id = now_ts();
    let phase = Phase {
        id,
        description: description.to_string(),
        messages: Vec::new(),
    };
    log.phases.push(phase);
    save_log(&log)?;
    Ok(id)
}

/// Add a message to an existing phase.
pub fn add_message(phase_id: u64, role: &str, content: &str) -> Result<()> {
    let mut log = load_log()?;
    if let Some(phase) = log.phases.iter_mut().find(|p| p.id == phase_id) {
        phase.messages.push(Message {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: now_ts(),
        });
        save_log(&log)?;
        Ok(())
    } else {
        anyhow::bail!("Phase id {} not found", phase_id);
    }
}

/// Retrieve all phases.
pub fn get_phases() -> Result<Vec<Phase>> {
    Ok(load_log()?.phases)
}

/// Retrieve a specific phase by id.
pub fn get_phase(id: u64) -> Result<Option<Phase>> {
    Ok(load_log()?.phases.into_iter().find(|p| p.id == id))
}
