//! mgc bench — benchmark install layers (C3)
//! Bật profile có sẵn (MAGICORE_WEB_PROFILE_INSTALL) + đo wall-time mỗi layer.
//! Không thêm crate profiling — tái dùng profile nhúng của adapter.

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct BenchArgs {
    /// Packages để add + install (như mgc install)
    #[arg(help = "package@version (multiple packages)")]
    packages: Vec<String>,

    #[arg(long, help = "force fresh resolve (skip lockfile)")]
    no_lock: bool,
}

pub async fn handle(args: BenchArgs) -> Result<()> {
    std::env::set_var("MAGICORE_WEB_PROFILE_INSTALL", "1");

    let started = std::time::Instant::now();

    if args.no_lock {
        // bỏ mgc.lock để đo resolve từ đầu — pointer: lock nằm cạnh package.json
        let lock = std::env::current_dir()?.join("mgc.lock");
        let _ = std::fs::remove_file(&lock);
    }

    crate::commands::install::run(args.packages, None, false, true, false).await?;

    let total_ms = started.elapsed().as_millis() as u64;
    eprintln!("[magicore:bench] total={}ms", total_ms);
    Ok(())
}
