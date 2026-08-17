//! mg-exec — exec passthrough + allowlist + audit sanitizer (MegaGate)
//! Forwards to trusted tools (allowlist), records exec audit with secret sanitization.
//! (Passthrough exec + allowlist + ghi audit có sanitize bí mật — sys-mg/00 §5, sys-mg/20 §3)

pub mod allowlist;
pub mod audit;
pub mod run;
pub mod sanitizer;

pub mod prelude {
    //! Mọi thứ adapter/core cần cho passthrough — một import duy nhất.
    pub use crate::allowlist::{
        check_tool, check_tool_scoped, find_forbidden_tool_in_script, parse_script_invocation,
        parse_simple_script, reject_forbidden_pm_script, reject_shell_control, ScriptInvocation,
        ALLOWED_TOOLS, FORBIDDEN_TOOLS,
    };
    pub use crate::audit::{append, AuditEntry};
    pub use crate::run::{
        process_tree_guard_available, run, run_inherited, run_project_binary,
        run_project_binary_inherited, ExecOptions, ExecReport,
    };
    pub use crate::sanitizer::redact_args;
}
