//! `launcher_policy.rs` — Shared Runtime Launcher Security Policy
//! `launcher_policy.rs` — Chính sách bảo mật Runtime Launcher dùng chung
//!
//! SAFETY: Centralized validation for Bun/Deno/Node runtime flags and arguments.
//! AN TOÀN: Kiểm tra tập trung cho flags và arguments của Bun/Deno/Node runtime.
//!
//! Used by: Web, AI, App, Lib cores (any core that launches external runtimes)
//! Dùng bởi: Core Web, AI, App, Lib (mọi core chạy runtime bên ngoài)

use anyhow::Result;

/// Runtime types that require policy validation
/// Loại runtime cần kiểm tra policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Bun,
    Deno,
    Node,
}

impl Runtime {
    pub fn name(&self) -> &'static str {
        match self {
            Runtime::Bun => "bun",
            Runtime::Deno => "deno",
            Runtime::Node => "node",
        }
    }

    /// Get list of dangerous flags that must be blocked
    /// Lấy danh sách flags nguy hiểm phải chặn
    fn dangerous_flags(&self) -> Vec<&'static str> {
        match self {
            Runtime::Bun => vec![
                "--eval", "-e", "--print",
                "-p",
                // Block direct file execution without project script
                // Chặn thực thi file trực tiếp không qua project script
            ],
            Runtime::Deno => vec![
                "--eval",
                "-e",
                "--allow-all",
                "-A",
                // Individual --allow-* flags checked separately
                // Các flags --allow-* riêng lẻ kiểm tra riêng
            ],
            Runtime::Node => vec!["--eval", "-e", "--print", "-p"],
        }
    }

    /// Get list of dangerous Deno permission flags that require explicit allowlist
    /// Lấy danh sách flags quyền Deno nguy hiểm cần allowlist rõ ràng
    fn deno_dangerous_permissions(&self) -> Vec<&'static str> {
        if matches!(self, Runtime::Deno) {
            vec![
                "--allow-read",
                "--allow-write",
                "--allow-net",
                "--allow-run",
                "--allow-ffi",
                "--allow-hrtime",
            ]
        } else {
            vec![]
        }
    }
}

/// Launcher policy configuration
/// Cấu hình policy launcher
#[derive(Debug, Clone)]
pub struct LauncherPolicy {
    /// Runtime type being validated
    /// Loại runtime đang kiểm tra
    pub runtime: Runtime,

    /// Whether to allow dangerous permissions (default: false)
    /// Cho phép quyền nguy hiểm hay không (mặc định: false)
    pub allow_dangerous_permissions: bool,

    /// Whether this is a DevServer context (more permissive than Install)
    /// Context DevServer hay không (dễ dãi hơn Install)
    pub is_dev_server: bool,
}

impl LauncherPolicy {
    /// Create policy for DevServer context (used by mgc dev)
    /// Tạo policy cho context DevServer (dùng bởi mgc dev)
    pub fn dev_server(runtime: Runtime) -> Self {
        Self {
            runtime,
            allow_dangerous_permissions: false,
            is_dev_server: true,
        }
    }

    /// Create policy for TestRunner context (used by mgc test)
    /// Tạo policy cho context TestRunner (dùng bởi mgc test)
    pub fn test_runner(runtime: Runtime) -> Self {
        Self {
            runtime,
            allow_dangerous_permissions: false,
            is_dev_server: false,
        }
    }

    /// Validate runtime arguments against policy
    /// Kiểm tra arguments runtime theo policy
    ///
    /// # Errors
    /// Returns error if dangerous flags detected or permissions not allowed
    /// Trả lỗi nếu phát hiện flags nguy hiểm hoặc quyền không được phép
    pub fn validate_args(&self, args: &[&str]) -> Result<()> {
        // Check dangerous flags (always blocked)
        // Kiểm tra flags nguy hiểm (luôn chặn)
        for arg in args {
            for flag in self.runtime.dangerous_flags() {
                if arg.starts_with(flag) {
                    return Err(crate::error::runtime_dangerous_flag_rejected(
                        self.runtime.name(),
                        flag,
                    ));
                }
            }
        }

        // Check Deno dangerous permissions (conditionally blocked)
        // Kiểm tra quyền nguy hiểm Deno (chặn có điều kiện)
        if matches!(self.runtime, Runtime::Deno) && !self.allow_dangerous_permissions {
            for arg in args {
                for perm in self.runtime.deno_dangerous_permissions() {
                    if arg.starts_with(perm) {
                        return Err(crate::error::runtime_dangerous_permission_rejected(
                            self.runtime.name(),
                            perm,
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bun_rejects_eval() {
        let policy = LauncherPolicy::dev_server(Runtime::Bun);
        assert!(policy.validate_args(&["--eval", "console.log(1)"]).is_err());
        assert!(policy.validate_args(&["-e", "console.log(1)"]).is_err());
        assert!(policy.validate_args(&["--print", "1"]).is_err());
    }

    #[test]
    fn test_deno_rejects_allow_all() {
        let policy = LauncherPolicy::dev_server(Runtime::Deno);
        assert!(policy.validate_args(&["--allow-all", "script.ts"]).is_err());
        assert!(policy.validate_args(&["-A", "script.ts"]).is_err());
    }

    #[test]
    fn test_deno_rejects_dangerous_permissions() {
        let policy = LauncherPolicy::dev_server(Runtime::Deno);
        assert!(policy
            .validate_args(&["--allow-read", "script.ts"])
            .is_err());
        assert!(policy
            .validate_args(&["--allow-write", "script.ts"])
            .is_err());
        assert!(policy.validate_args(&["--allow-net", "script.ts"]).is_err());
        assert!(policy.validate_args(&["--allow-run", "script.ts"]).is_err());
    }

    #[test]
    fn test_deno_allows_safe_flags() {
        let policy = LauncherPolicy::dev_server(Runtime::Deno);
        assert!(policy.validate_args(&["run", "script.ts"]).is_ok());
        assert!(policy.validate_args(&["task", "dev"]).is_ok());
    }

    #[test]
    fn test_bun_allows_safe_args() {
        let policy = LauncherPolicy::dev_server(Runtime::Bun);
        assert!(policy.validate_args(&["run", "server.ts"]).is_ok());
        assert!(policy.validate_args(&["test"]).is_ok());
    }

    #[test]
    fn test_node_rejects_eval() {
        let policy = LauncherPolicy::dev_server(Runtime::Node);
        assert!(policy.validate_args(&["--eval", "console.log(1)"]).is_err());
        assert!(policy.validate_args(&["-e", "console.log(1)"]).is_err());
    }
}
