use crate::context::ProjectContext;
use anyhow::Result;
use mg_types::adapter::AddOptions;

/// mg add — add a dependency to the project
#[allow(dead_code)]
pub async fn run(
    package: String,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
    core: Option<&str>,
) -> Result<()> {
    run_many(
        vec![package],
        version,
        dev,
        exact,
        optional,
        peer,
        no_save,
        global,
        core,
    )
    .await
}

pub async fn run_many(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
    core: Option<&str>,
) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();
    let total = packages.len();

    let group = if peer {
        "peerDependencies"
    } else if optional {
        "optionalDependencies"
    } else if dev {
        "devDependencies"
    } else {
        "dependencies"
    };
    mg_ui::info(&format!(
        "Adding {} package(s) to {} ({})...",
        total, ctx.config.name, group
    ));

    for package in packages {
        let spec = mg_types::DependencySpec::parse(&package)?;
        let name = spec.name;
        let range = if let Some(v) = version.as_ref() {
            Some(mg_types::VersionRange::parse(v)?)
        } else if spec.range.is_star() {
            None
        } else {
            Some(spec.range)
        };

        let spinner = mg_ui::create_spinner(&format!("  Resolving {}...", package));
        let opts = AddOptions {
            dev,
            optional,
            peer,
            exact,
            no_save,
            global,
        };
        let pkg_id = adapter.add(ctx.root(), &name, range.as_ref(), opts).await?;
        spinner.finish_and_clear();
        let requested_range = range
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "*".to_string());
        let resolved_version = pkg_id.version().to_string();

        if !no_save {
            if resolved_version == "0.0.0" {
                mg_ui::info(&format!(
                    "  {}@{} saved to {}",
                    pkg_id.name_str(),
                    requested_range,
                    group
                ));
            } else {
                mg_ui::info(&format!(
                    "  {}@{} added to {}",
                    pkg_id.name_str(),
                    resolved_version,
                    group
                ));
            }
            mg_ui::success(&format!("Added {}", package));
        } else {
            mg_ui::info(&format!(
                "  {}@{} checked (--no-save, manifest unchanged)",
                pkg_id.name_str(),
                requested_range
            ));
        }
    }

    if !no_save {
        mg_ui::info(&format!(
            "Run '{}' to install",
            mg_ui::style_cmd("mg install")
        ));
    }

    Ok(())
}
