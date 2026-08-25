//! mgc trust list — List keys in keyring
//! mgc trust list — Liệt kê keys trong keyring

use mgc_crypto::keyring::Keyring;

/// Execute `mgc trust list` — Thực thi `mgc trust list`
pub fn execute() -> anyhow::Result<()> {
    let keyring_path = Keyring::default_path();

    // Check if keyring exists
    if !keyring_path.exists() {
        println!("⚠ No keyring found");
        println!("  Run 'mgc trust init' to create one");
        return Ok(());
    }

    // Load keyring
    let keyring = Keyring::load(&keyring_path)?;

    if keyring.keys.is_empty() {
        println!("⚠ Keyring is empty");
        return Ok(());
    }

    println!("Keys in keyring:");
    println!("  Location: {}\n", keyring_path.display());

    for key in &keyring.keys {
        let is_default = keyring
            .default_key_id
            .as_ref()
            .map_or(false, |id| id == &key.key_id);

        let marker = if is_default { "*" } else { " " };

        println!("{} Key ID: {}", marker, key.key_id);
        println!("  Public key: {}...", &key.public_key.to_base64()[..20]);
        println!("  Created: {}", format_timestamp(key.created_at));
        println!();
    }

    if let Some(default_id) = &keyring.default_key_id {
        println!("* = default key ({})", default_id);
    }

    Ok(())
}

/// Format Unix timestamp to human-readable — Format timestamp Unix sang dễ đọc
fn format_timestamp(ts: u64) -> String {
    use chrono::{DateTime, Utc};
    let dt = DateTime::<Utc>::from_timestamp(ts as i64, 0).unwrap_or_default();
    dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}
