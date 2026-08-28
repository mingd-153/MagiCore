//! Cloud infrastructure scaffolding.

use crate::cloud_type::CloudType;
use mgc_types::MgResult;
use std::path::Path;

pub async fn scaffold_project(framework: CloudType, name: &str, dir: &Path) -> MgResult<()> {
    std::fs::create_dir_all(dir)?;

    match framework {
        CloudType::Cdk => scaffold_cdk(name, dir).await,
        CloudType::Pulumi => scaffold_pulumi(name, dir).await,
        CloudType::Terraform => scaffold_terraform(name, dir).await,
        CloudType::Cloudflare => scaffold_cloudflare(name, dir).await,
    }
}

async fn scaffold_cloudflare(name: &str, dir: &Path) -> MgResult<()> {
    let toml = format!("name=\"{}\"\ncompatibility_date=\"2024-01-01\"\n", name);
    std::fs::write(dir.join("wrangler.toml"), toml)?;
    Ok(())
}

async fn scaffold_cdk(name: &str, dir: &Path) -> MgResult<()> {
    let pkg = format!(
        "{{\"name\":\"{}\",\"version\":\"0.1.0\",\"dependencies\":{{\"aws-cdk-lib\":\"^2.0.0\"}}}}",
        name
    );
    std::fs::write(dir.join("package.json"), pkg)?;

    std::fs::create_dir_all(dir.join("bin"))?;
    std::fs::write(
        dir.join("bin/app.ts"),
        "import * as cdk from 'aws-cdk-lib';\n",
    )?;
    Ok(())
}

async fn scaffold_pulumi(name: &str, dir: &Path) -> MgResult<()> {
    let yaml = format!("name: {}\nruntime: nodejs\n", name);
    std::fs::write(dir.join("Pulumi.yaml"), yaml)?;

    let pkg = format!(
        "{{\"name\":\"{}\",\"dependencies\":{{\"@pulumi/aws\":\"^6.0.0\"}}}}",
        name
    );
    std::fs::write(dir.join("package.json"), pkg)?;
    Ok(())
}

async fn scaffold_terraform(name: &str, dir: &Path) -> MgResult<()> {
    let tf = format!("terraform {{\n  required_version = \">= 1.0\"\n}}\n\nresource \"null_resource\" \"{}\" {{}}\n", name);
    std::fs::write(dir.join("main.tf"), tf)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
