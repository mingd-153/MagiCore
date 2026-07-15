use anyhow::{bail, Result};

use crate::context::ProjectContext;

pub async fn run(core: Option<&str>, host: Option<String>, port: Option<u16>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;

    match ctx.adapter().name() {
        "web" => crate::commands::core::web::dev_at_root(ctx.root(), host, port).await,
        other => bail!("'mg dev' is not implemented for the '{other}' core yet"),
    }
}
