use anyhow::Result;
use std::time::Duration;
use mg_ui::{success, info, create_spinner, create_progress_bar, create_multi_progress, add_multi_bar, print_install_summary, style_cmd};
use crate::context::ProjectContext;

/// mg install — install dependencies for the current project
pub async fn run(packages: Vec<String>, core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    if !packages.is_empty() {
        for pkg in &packages {
            let spinner = create_spinner(&format!("  Adding {}...", pkg));
            tokio::time::sleep(Duration::from_millis(200)).await;

            let name = mg_types::PackageName::new(pkg)?;
            adapter.add(ctx.root(), &name, None, false, false, false, false, false, false).await?;
            spinner.finish_and_clear();
        }
    }

    let spinner = create_spinner("  Reading project manifest...");
    tokio::time::sleep(Duration::from_millis(150)).await;
    let manifest = adapter.parse_manifest(ctx.root()).await?;
    spinner.finish_and_clear();

    let all_deps: Vec<_> = manifest.all_dependencies().collect();
    if all_deps.is_empty() {
        info("No dependencies to install.");
        info(&format!("Use '{} <package>' to add dependencies.", style_cmd("mg add")));
        return Ok(());
    }

    let spinner = create_spinner(&format!("  Resolving {} dependencies...", all_deps.len()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let graph = adapter.resolve(&manifest).await?;
    spinner.finish_and_clear();

    let resolve_bar = create_progress_bar(graph.len() as u64, "Resolving...");
    resolve_bar.finish_with_message(format!("✅  Resolved {} packages", graph.len()));

    let multi = create_multi_progress();
    let mut bars = vec![];
    for pkg in &graph.packages {
        let pb = add_multi_bar(&multi, 100, &format!("{}@{}", pkg.id.name_str(), pkg.id.version()));
        bars.push(pb);
    }

    for (i, pb) in bars.iter().enumerate() {
        for _ in 0..100 {
            pb.inc(1);
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        pb.finish_with_message(format!("✅ {}", graph.packages[i].id.name_str()));
    }

    let spinner = create_spinner("  Linking packages...");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let summary = adapter.install(&graph, ctx.root()).await?;
    spinner.finish_and_clear();

    print_install_summary(
        summary.added.len(),
        summary.bytes_from_cache as usize,
        summary.duration_ms,
        "0 B",
    );

    println!();
    success("All dependencies installed");
    Ok(())
}
