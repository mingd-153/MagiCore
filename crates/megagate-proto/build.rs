use prost_build::Config;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::new();
    config
        .out_dir("src")
        .compile_protos(&["proto/megagate/v1/package.proto"], &["proto/"])?;

    println!("cargo:rerun-if-changed=proto/megagate/v1/package.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/resolver.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/linker.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/extractor.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/fetcher.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/security.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/store.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/lockfile.proto");
    println!("cargo:rerun-if-changed=proto/megagate/v1/common.proto");

    Ok(())
}