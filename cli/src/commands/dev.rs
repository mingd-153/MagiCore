use anyhow::bail;
use mg_ui::info;

use crate::context::ProjectContext;

pub async fn run(
    core: Option<&str>,
    host: Option<String>,
    port: Option<u16>,
    clear: bool,
) -> anyhow::Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let host = host.unwrap_or_else(|| "127.0.0.1".to_string());
    let root = ctx.root().to_path_buf();

    info("Starting MegaGate Native Dev Server...");
    info(&format!("Project root: {}", root.display()));

    match ctx.adapter().name() {
        "web" => {
            if clear {
                info("--clear is delegated to the selected web framework when supported.");
            }
            crate::commands::core::web::dev_at_root(&root, Some(host), port).await
        }
        "game" => {
            info("Engine Game: Starting native watcher and game runtime...");
            Ok(())
        }
        other => bail!("'mg dev' Engine is not implemented for the '{other}' core yet"),
    }
}
