// Windows CLI stack sizing — reserve enough main-thread stack for the full capability graph.
// Cấp stack thread chính cho CLI Windows — đủ để dựng graph capability của mọi core.

const WINDOWS_CLI_STACK_SIZE: &str = "8388608";

fn main() {
    println!("cargo:rerun-if-env-changed=TARGET");
    let target = std::env::var("TARGET").unwrap_or_default();
    let linker_arg = if target.ends_with("-windows-msvc") {
        Some(format!("/STACK:{WINDOWS_CLI_STACK_SIZE}"))
    } else if target.ends_with("-windows-gnu") {
        Some(format!("-Wl,--stack,{WINDOWS_CLI_STACK_SIZE}"))
    } else {
        None
    };

    if let Some(arg) = linker_arg {
        println!("cargo:rustc-link-arg-bin=mgc={arg}");
    }
}
