use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CONV_FILE: &str = "conversation.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Conversation {
    pub messages: Vec<Message>,
}

fn conv_path() -> PathBuf {
    // Store next to the project root (current working directory)
    let mut path = std::env::current_dir().expect("cannot get cwd");
    path.push(CONV_FILE);
    path
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load_conv() -> Result<Conversation> {
    let path = conv_path();
    if path.exists() {
        let mut data = String::new();
        fs::File::open(&path)?.read_to_string(&mut data)?;
        let conv: Conversation = serde_json::from_str(&data)?;
        Ok(conv)
    } else {
        Ok(Conversation::default())
    }
}

fn save_conv(conv: &Conversation) -> Result<()> {
    let path = conv_path();
    let json = serde_json::to_string_pretty(conv)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

/// Append a new message (role: "user" or "agent") to the persisted conversation.
pub fn append_message(role: &str, content: &str) -> Result<()> {
    let mut conv = load_conv()?;
    conv.messages.push(Message {
        role: role.to_string(),
        content: content.to_string(),
        timestamp: now_ts(),
    });
    save_conv(&conv)
}

/// Retrieve the whole conversation.
pub fn get_conversation() -> Result<Conversation> {
    load_conv()
}

/// Get a file URI that can be used to link to the stored conversation.
pub fn conversation_uri() -> String {
    let path = conv_path();
    format!("file://{}", path.to_string_lossy())
}
