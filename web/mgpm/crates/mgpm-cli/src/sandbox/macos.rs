use super::SandboxGuard;
use std::path::Path;

#[cfg(target_os = "macos")]
pub fn apply_seatbelt(project_dir: &Path) -> Result<SandboxGuard, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let profile = format!(
        "(version 1)
        (deny default)
        (allow file-read* (subpath \"/Users\"))
        (allow file-write* (subpath \"{home}/.mgpm\"))
        (allow file-write* (subpath \"{}/node_modules\"))
        (allow network-outbound)
        (allow process-exec (literal \"/usr/bin/env\") (literal \"/bin/sh\"))
        (deny process-fork)",
        project_dir.display()
    );

    // Write profile to temp file
    let profile_path = std::env::temp_dir().join(format!("mgpm-sandbox-{}.sb", std::process::id()));
    std::fs::write(&profile_path, &profile).map_err(|e| e.to_string())?;

    // Apply sandbox using sandbox-init (requires entitlement on hardened runtime)
    // For now, just write the profile and instruct user
    eprintln!("Sandbox profile written to {:?}", profile_path);
    eprintln!("To apply: sandbox-exec -f {:?} mgpm install", profile_path);

    Ok(SandboxGuard {})
}
