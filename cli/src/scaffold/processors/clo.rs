//! Clo scaffold: terraform/cdk/cloudflare/lambda/pulumi templates.

use std::path::Path;

use anyhow::Result;

use super::{slugify, write_file};

pub struct CloProcessor;

impl CloProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "terraform" | "terraform-gcp" => write_file(
                &target.join("main.tf"),
                "terraform {\n  required_version = \">= 1.5.0\"\n}\n\nprovider \"google\" {}\n",
            )?,
            "cdk" | "cdk-typescript" => {
                write_file(
                    &target.join("package.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"scripts\": {{\n    \"synth\": \"cdk synth\"\n  }}\n}}\n",
                        name
                    ),
                )?;
                write_file(
                    &target.join("bin").join("app.ts"),
                    "console.log('MagiCore CDK app scaffold');\n",
                )?;
            }
            "cloudflare" => write_file(
                &target.join("wrangler.toml"),
                &format!("name = \"{}\"\nmain = \"src/index.ts\"\n", slugify(name)),
            )?,
            "lambda" => write_file(
                &target.join("handler.ts"),
                "export const handler = async () => ({ statusCode: 200, body: 'ok' });\n",
            )?,
            _ => {
                write_file(
                    &target.join("Pulumi.yaml"),
                    &format!(
                        "name: {}\nruntime: nodejs\ndescription: MagiCore cloud project\n",
                        slugify(name)
                    ),
                )?;
                write_file(
                    &target.join("package.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\"\n}}\n",
                        name
                    ),
                )?;
                write_file(
                    &target.join("index.ts"),
                    "console.log('MagiCore Pulumi scaffold');\n",
                )?;
            }
        }

        Ok(())
    }
}
