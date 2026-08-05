//! mg-exec — exec passthrough + allowlist + audit sanitizer (MegaGate)
//! Forwards to trusted tools (allowlist), records exec audit with secret sanitization.
//! (Passthrough exec + allowlist + ghi audit có sanitize bí mật — sys-mg/00 §5, sys-mg/20 §3)

pub mod allowlist;
pub mod audit;
pub mod run;
pub mod sanitizer;

pub mod prelude {
    //! Mọi thứ adapter/core cần cho passthrough — một import duy nhất.
    pub use crate::allowlist::{check_tool, ALLOWED_TOOLS, FORBIDDEN_TOOLS};
    pub use crate::audit::{append, AuditEntry};
    pub use crate::run::{run, ExecOptions, ExecReport};
    pub use crate::sanitizer::redact_args;
}