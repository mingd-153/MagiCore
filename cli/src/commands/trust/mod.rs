//! Trust commands for lockfile signing and verification
//! Lệnh trust cho ký và xác minh lockfile

pub mod approve;
pub mod deny;
pub mod init;
pub mod list;
pub mod policy;
pub mod prune;
pub mod sign;
pub mod verify;

use clap::Subcommand;

/// Trust subcommands — Lệnh con trust
#[derive(Debug, Clone, Subcommand)]
pub enum TrustCmd {
    /// Initialize keyring — Khởi tạo keyring
    Init {
        /// Force reinitialize — Buộc khởi tạo lại
        #[arg(long)]
        force: bool,
    },

    /// Sign lockfile — Ký lockfile
    Sign {
        /// Lockfile path — Đường dẫn lockfile
        #[arg(default_value = "mgc.lock")]
        lockfile: String,

        /// Key ID to use — Key ID để dùng
        #[arg(long)]
        key_id: Option<String>,
    },

    /// Verify lockfile — Xác minh lockfile
    Verify {
        /// Lockfile path — Đường dẫn lockfile
        #[arg(default_value = "mgc.lock")]
        lockfile: String,
    },

    /// List keys — Liệt kê keys
    List,

    /// Approve package lifecycle scripts — Cho phép lifecycle scripts của package
    Approve {
        /// Package name — Tên package
        package: String,
    },

    /// Deny package lifecycle scripts — Từ chối lifecycle scripts của package
    Deny {
        /// Package name — Tên package
        package: String,
    },

    /// Prune stale trust policies — Dọn policy cũ (package đã gỡ)
    Prune,
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
    }
}

/// Async wrapper for CLI dispatch — Wrapper async cho CLI dispatch
pub async fn run(cmd: TrustCmd) -> anyhow::Result<()> {
    execute(cmd)
}
