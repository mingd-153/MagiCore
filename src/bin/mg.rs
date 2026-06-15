fn main() {
    // Simply invoke the library's main function
    // This binary serves as a short alias `mg`
    // It calls the same entry point as the default binary.
    // Note: the actual implementation resides in src/main.rs
    // We delegate to the real main by calling the generated function.
    // This requires the crate to expose a public entry point.
    // We'll use the same Cargo package binary name `megagate`.
    // The `megagate` binary is built from src/main.rs, so we just call that.
    // Since Rust does not allow calling main directly, we re-export it.
    // We'll create a public function `run` in src/main.rs for reuse.
    // For now, we simply invoke the binary via std::process::Command.
    let status = std::process::Command::new("./target/debug/megagate")
        .args(std::env::args().skip(1))
        .status()
        .expect("failed to execute megagate");
    std::process::exit(status.code().unwrap_or(1));
}
