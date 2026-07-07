fn main() {
    let mg_core_c = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ).parent().unwrap().parent().unwrap().join("crates/mg-core-c");

    let include = mg_core_c.join("include");
    let src = mg_core_c.join("src");

    cc::Build::new()
        .file(src.join("semver.c"))
        .file(src.join("json_extract.c"))
        .file(src.join("sha256.c"))
        .file(src.join("tar_extract.c"))
        .include(&include)
        .warnings_into_errors(true)
        .compile("mg_core_c");

    // Link system zlib for tar_extract.c
    println!("cargo:rustc-link-lib=z");
}
