//! Trust commands for lockfile signing and verification
//! Lệnh trust cho ký và xác minh lockfile

pub mod approve;
pub mod deny;
pub mod init;
pub mod list;
pub mod pending;
pub mod policy;
pub mod prune;
pub mod sign;
pub mod verify;

use clap::Subcommand;

/// Trust subcommands (help text English — RULE §7).
/// Lệnh con trust (text help tiếng Anh — RULE §7).
#[derive(Debug, Clone, Subcommand)]
pub enum TrustCmd {
    /// Initialize keyring
    Init {
        /// Force reinitialize
        #[arg(long)]
        force: bool,
    },

    /// Sign lockfile
    Sign {
        /// Lockfile path
        #[arg(default_value = "mgc.lock")]
        lockfile: String,

        /// Key ID to use
        #[arg(long)]
        key_id: Option<String>,
    },

    /// Verify lockfile
    Verify {
        /// Lockfile path
        #[arg(default_value = "mgc.lock")]
        lockfile: String,
    },

    /// List keys
    List,

    /// Approve package lifecycle scripts
    Approve {
        /// Package name
        package: String,
    },

    /// Deny package lifecycle scripts
    Deny {
        /// Package name
        package: String,
    },

    /// Prune stale trust policies
    Prune,

    /// List installed packages with lifecycle scripts but no policy yet
    Pending,
}

/// Execute trust command — Thực thi lệnh trust
pub fn execute(cmd: TrustCmd) -> anyhow::Result<()> {
    match cmd {
        TrustCmd::Init { force } => init::execute(force),
        TrustCmd::Sign { lockfile, key_id } => sign::execute(&lockfile, key_id.as_deref()),
        TrustCmd::Verify { lockfile } => verify::execute(&lockfile),
        TrustCmd::List => list::execute(),
        TrustCmd::Approve { package } => approve::execute(&package),
        TrustCmd::Deny { package } => deny::execute(&package),
        TrustCmd::Prune => prune::execute(),
        TrustCmd::Pending => pending::execute(),
    }
}

/// Async wrapper for CLI dispatch — Wrapper async cho CLI dispatch
pub async fn run(cmd: TrustCmd) -> anyhow::Result<()> {
    execute(cmd)
}
