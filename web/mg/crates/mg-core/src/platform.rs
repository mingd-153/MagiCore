use std::env::consts;

const NPM_OSS: &[&str] = &[
    "darwin", "linux", "win32", "android", "freebsd",
    "openbsd", "netbsd", "sunos", "aix", "cygwin", "haiku",
];

const NPM_ARCHES: &[&str] = &[
    "arm64", "x64", "x86", "ia32", "arm", "s390x",
    "ppc64", "riscv64", "loong64", "mips", "mipsel",
];

fn rust_os_to_npm() -> &'static str {
    match consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        "illumos" | "solaris" => "sunos",
        other => other,
    }
}

fn rust_arch_to_npm() -> &'static str {
    match consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        "powerpc64" => "ppc64",
        "loongarch64" => "loong64",
        other => other,
    }
}

pub fn current_os() -> &'static str {
    rust_os_to_npm()
}

pub fn current_arch() -> &'static str {
    rust_arch_to_npm()
}

pub fn is_platform_match(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    let base = name_lower.rsplit('/').next().unwrap_or(&name_lower);

    let os = rust_os_to_npm();
    let arch = rust_arch_to_npm();

    let os_any = NPM_OSS.iter().any(|n| base.contains(*n));
    let arch_any = NPM_ARCHES.iter().any(|n| base.contains(*n));
    let os_match = base.contains(os);
    let arch_match = base.contains(arch);

    match (os_any, arch_any) {
        (false, false) => true,
        (true, false) => os_match,
        (false, true) => {
            let has_standard_form = NPM_ARCHES.iter().any(|a| base.ends_with(&format!("-{a}")));
            if has_standard_form {
                false
            } else {
                true
            }
        }
        (true, true) => os_match && arch_match,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform_non_empty() {
        assert!(!current_os().is_empty());
        assert!(!current_arch().is_empty());
    }

    #[test]
    fn test_platform_match_non_platform_pkg() {
        assert!(is_platform_match("react"));
        assert!(is_platform_match("lodash"));
        assert!(is_platform_match("express"));
        assert!(is_platform_match("fsevents"));
    }

    #[test]
    fn test_platform_match_scoped_non_platform() {
        assert!(is_platform_match("@types/react"));
        assert!(is_platform_match("@angular/core"));
    }

    #[test]
    fn test_platform_match_esbuild_current() {
        let os = current_os();
        let arch = current_arch();
        let name = format!("@esbuild/{os}-{arch}");
        assert!(is_platform_match(&name));
    }

    #[test]
    fn test_platform_match_esbuild_other_platform() {
        assert!(!is_platform_match("@esbuild/win32-x64"));
        assert!(!is_platform_match("@esbuild/linux-arm64"));
        assert!(!is_platform_match("@esbuild/linux-x64"));
        assert!(!is_platform_match("@esbuild/linux-arm"));
        assert!(!is_platform_match("@esbuild/linux-ia32"));
        assert!(!is_platform_match("@esbuild/linux-loong64"));
        assert!(!is_platform_match("@esbuild/linux-mips64el"));
        assert!(!is_platform_match("@esbuild/linux-ppc64"));
        assert!(!is_platform_match("@esbuild/linux-riscv64"));
        assert!(!is_platform_match("@esbuild/linux-s390x"));
        assert!(!is_platform_match("@esbuild/freebsd-arm64"));
        assert!(!is_platform_match("@esbuild/netbsd-arm64"));
        assert!(!is_platform_match("@esbuild/openbsd-x64"));
        assert!(!is_platform_match("@esbuild/sunos-x64"));
        assert!(!is_platform_match("@esbuild/android-arm64"));
        assert!(!is_platform_match("@esbuild/haiku-x64"));
    }

    #[test]
    fn test_platform_match_exotic_os() {
        assert!(!is_platform_match("@esbuild/openharmony-arm64"));
    }

    #[test]
    fn test_platform_match_rollup_current() {
        let os = current_os();
        let arch = current_arch();
        let name = format!("@rollup/rollup-{os}-{arch}");
        assert!(is_platform_match(&name));
    }

    #[test]
    fn test_platform_match_rollup_other() {
        assert!(!is_platform_match("@rollup/rollup-linux-x64-gnu"));
        assert!(!is_platform_match("@rollup/rollup-linux-arm64-gnu"));
        assert!(!is_platform_match("@rollup/rollup-linux-arm-gnueabihf"));
        assert!(!is_platform_match("@rollup/rollup-win32-x64-msvc"));
    }
}
