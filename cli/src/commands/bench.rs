//! mg bench — benchmark install layers (C3)
//! Bật profile có sẵn (MEGAGATE_WEB_PROFILE_INSTALL) + đo wall-time mỗi layer.
//! Không thêm crate profiling — tái dùng profile nhúng của adapter.

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct BenchArgs {
    /// Packages để add + install (như mg install)
    #[arg(help = "package@version (nhiều package)")]
    packages: Vec<String>,

    #[arg(long, help = "ép resolve mới (bỏ lockfile)")]
    no_lock: bool,
}

pub async fn handle(args: BenchArgs) -> Result<()> {
    std::env::set_var("MEGAGATE_WEB_PROFILE_INSTALL", "1");

    let started = std::time::Instant::now();

    if args.no_lock {
        // bỏ mg.lock để đo resolve từ đầu — pointer: lock nằm cạnh package.json
        let lock = std::env::current_dir()?.join("mg.lock");
        let _ = std::fs::remove_file(&lock);
    }

    crate::commands::install::run(args.packages, None, false, true).await?;

    let total_ms = started.elapsed().as_millis() as u64;
    eprintln!("[megagate:bench] total={}ms", total_ms);
    Ok(())
}
