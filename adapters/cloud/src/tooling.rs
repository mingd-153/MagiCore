//! Cloud native tooling helpers.
//! Gom phần exec Terraform để adapter chính ngắn và rõ.

use mgc_types::MgResult;
use std::path::Path;

pub(crate) fn exec_tool(root: &Path, cmd: &str, args: &[String]) -> MgResult<()> {
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mgc_exec::prelude::run(cmd, args, &opts)
        .map_err(|e| mgc_types::MgError::Other(e.to_string()))?;
    Ok(())
}
