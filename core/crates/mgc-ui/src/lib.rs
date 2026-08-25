#![cfg_attr(test, allow(clippy::unwrap_used))]

use console::style;
/// Terminal UI components for MagiCore
///
/// Provides progress bars, spinners, interactive prompts, and styled output.
/// Uses indicatif + console for rich terminal experience.
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::atomic::{AtomicBool, Ordering};

pub mod help;
pub mod progress;
pub mod prompt;
pub mod table;

static QUIET: AtomicBool = AtomicBool::new(false);

pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// MagiCore banner shown at wizard start
pub fn print_banner() {
    if is_quiet() {
        return;
    }
    let banner = style(
        r#"
    ╔══════════════════════════════════════════╗
    ║           🚀  MagiCore  🚀              ║
    ║      Universal Package Manager           ║
    ╚══════════════════════════════════════════╝
    "#,
    )
    .cyan()
    .bold();
    println!("{}", banner);
}

/// Print a section header
pub fn section(title: &str, current: usize, total: usize) {
    if is_quiet() {
        return;
    }
    println!();
    println!(
        "  {} {}",
        style("◆").cyan().bold(),
        style(format!("Step {}/{}", current, total)).dim()
    );
    println!("  {} {}", style("┃").cyan(), style(title).bold().white());
    println!("  {} {}", style("┃").cyan(), style("─".repeat(40)).dim());
}

/// Create a styled progress bar
pub fn create_progress_bar(len: u64, msg: &str) -> ProgressBar {
    if is_quiet() {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] {bar:30.cyan/blue} {pos:>3}/{len:3} {msg:.white}")
            .expect("valid progress template")
            .progress_chars("━╾─"),
    );
    pb.set_message(msg.to_string());
    pb
}

/// Create a multi progress bar group (for parallel downloads)
pub fn create_multi_progress() -> MultiProgress {
    MultiProgress::new()
}

/// Add a progress bar to a multi progress group
pub fn add_multi_bar(multi: &MultiProgress, len: u64, msg: &str) -> ProgressBar {
    if is_quiet() {
        return ProgressBar::hidden();
    }
    let pb = multi.add(ProgressBar::new(len));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  {spinner:.green} {msg:25.cyan/blue} {bar:20.cyan/blue} {bytes:>7}/{total_bytes:7} {eta}")
            .expect("valid progress template")
            .progress_chars("━╾─"),
    );
    pb.set_message(msg.to_string());
    pb
}

/// Create a spinner
pub fn create_spinner(msg: &str) -> ProgressBar {
    if is_quiet() {
        return ProgressBar::hidden();
    }
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("valid progress template")
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    spinner.set_message(msg.to_string());
    spinner
}

/// Print a success message with checkmark
pub fn success(msg: &str) {
    if is_quiet() {
        return;
    }
    println!("  {} {}", style("✔").green().bold(), msg);
}

/// Print an error message
pub fn error(msg: &str) {
    eprintln!("  {} {}", style("✘").red().bold(), msg);
}

/// Print a warning message
pub fn warning(msg: &str) {
    if is_quiet() {
        return;
    }
    println!("  {} {}", style("⚠").yellow().bold(), msg);
}

/// Print an info message
pub fn info(msg: &str) {
    if is_quiet() {
        return;
    }
    println!("  {} {}", style("ℹ").blue(), msg);
}

/// Print a dimmed hint
pub fn hint(msg: &str) {
    if is_quiet() {
        return;
    }
    println!("  {}", style(msg).dim());
}

pub fn blank_line() {
    if is_quiet() {
        return;
    }
    println!();
}

/// Style a command name for display (e.g., `mgc init` in cyan)
pub fn style_cmd(cmd: &str) -> String {
    format!("{}", style(cmd).cyan().bold())
}

/// Print next steps after project creation
pub fn print_next_steps(project_name: &str) {
    if is_quiet() {
        return;
    }
    println!();
    println!(
        "  {}",
        style("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓").green()
    );
    println!(
        "  {}",
        style("┃           ✅  All done!                   ┃")
            .green()
            .bold()
    );
    println!(
        "  {}",
        style("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛").green()
    );
    println!();
    success("Project created successfully!");
    println!();
    hint(format!("  cd {}", project_name).as_str());
    hint("  mgc install    # Install dependencies");
    hint("  mgc dev        # Start development");
}

/// Print a summary table after install
pub fn print_install_summary(added: usize, cached: usize, duration_ms: u64, disk_saved: &str) {
    if is_quiet() {
        return;
    }
    println!();
    println!(
        "  {}",
        style("┌──────────────────────────────────────────────┐").cyan()
    );
    println!(
        "  {}",
        style("│           📦  Install Complete               │")
            .cyan()
            .bold()
    );
    println!(
        "  {}",
        style("├──────────────────────────────────────────────┤").cyan()
    );
    println!("  {} {:>3} packages installed", style("│").cyan(), added);
    println!("  {} {:>3} from cache", style("│").cyan(), cached);
    println!("  {} {:>5} ms total", style("│").cyan(), duration_ms);
    println!(
        "  {} {:>10} saved (CAS dedup)",
        style("│").cyan(),
        style(disk_saved).green().bold()
    );
    println!(
        "  {}",
        style("└──────────────────────────────────────────────┘").cyan()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_creation() {
        let pb = create_progress_bar(100, "test");
        assert_eq!(pb.length().unwrap(), 100);
    }
}
