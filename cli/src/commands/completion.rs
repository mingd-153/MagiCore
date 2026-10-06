//! `mgc completion <shell>` — print shell completions to stdout.
//! (In script hoàn thiện shell ra stdout — eval hoặc redirect vào rc file.)
//!
//! Competitor parity (pnpm/bun/moon/turbo all ship this): uses the
//! already-declared `clap_complete` dependency against the real CLI
//! schema, so completions can never drift from the commands.
//! (Dùng clap_complete với schema CLI thật — không bao giờ lệch lệnh.)

use anyhow::Result;
use clap::{CommandFactory, ValueEnum};

/// Shells clap_complete can generate (lowercase CLI spelling).
/// (Shell hỗ trợ — tên CLI viết thường.)
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
}

pub fn handle(shell: CompletionShell) -> Result<()> {
    let generator = match shell {
        CompletionShell::Bash => clap_complete::Shell::Bash,
        CompletionShell::Zsh => clap_complete::Shell::Zsh,
        CompletionShell::Fish => clap_complete::Shell::Fish,
        CompletionShell::Powershell => clap_complete::Shell::PowerShell,
        CompletionShell::Elvish => clap_complete::Shell::Elvish,
    };
    let mut cmd = crate::Cli::command();
    clap_complete::generate(generator, &mut cmd, "mgc", &mut std::io::stdout());
    Ok(())
}
