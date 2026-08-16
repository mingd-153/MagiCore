//! mg sbom — generate a CycloneDX 1.5 BOM from the resolved graph (T5 §3.3).
//! (Dùng graph đã resolve, không fetch lại. Fail-đóng: không resolve được → lỗi.)

use anyhow::Result;
use serde::Serialize;
use std::collections::HashSet;

use crate::context::ProjectContext;
use mg_types::adapter::ResolvedPackage;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CycloneDXComponent {
    #[serde(rename = "type")]
    component_type: &'static str,
    #[serde(rename = "bom-ref")]
    bom_ref: String,
    name: String,
    version: String,
    purl: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CycloneDXBom {
    bom_format: &'static str,
    spec_version: &'static str,
    version: u32,
    components: Vec<CycloneDXComponent>,
}

/// npm package-url: scoped names encode '@' as '%40'
/// (pkg:npm/%40scope/name@1.0.0). Component bom-ref = the purl (unique).
fn purl_for(pkg: &ResolvedPackage) -> String {
    let name = pkg.id.name_str().replace('@', "%40");
    format!("pkg:npm/{name}@{}", pkg.id.version())
}

/// mg sbom — write a CycloneDX 1.5 BOM for the project dependency graph.
pub async fn run(core: Option<&str>, output: Option<String>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let manifest = ctx.adapter().parse_manifest(ctx.root()).await?;
    let graph = ctx.adapter().resolve(&manifest).await?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut components = Vec::with_capacity(graph.len());
    for pkg in &graph.packages {
        let name = pkg.id.name_str().to_string();
        let version = pkg.id.version().to_string();
        let key = format!("{name}@{version}");
        if !seen.insert(key.clone()) {
            mg_ui::warning(&format!(
                "duplicate component {key} — duplicate bom-ref skipped"
            ));
            continue;
        }
        let purl = purl_for(pkg);
        components.push(CycloneDXComponent {
            component_type: "library",
            bom_ref: purl.clone(),
            name,
            version,
            purl,
        });
    }

    let bom = CycloneDXBom {
        bom_format: "CycloneDX",
        spec_version: "1.5",
        version: 1,
        components,
    };

    let json = serde_json::to_string_pretty(&bom)?;
    match output {
        Some(path) => {
            std::fs::write(&path, &json)?;
            println!(
                "SBOM (CycloneDX 1.5) — {} unique component(s) → {path}",
                bom.components.len()
            );
        }
        None => println!("{json}"),
    }
    Ok(())
}
