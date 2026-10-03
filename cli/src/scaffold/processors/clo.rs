//! Clo scaffold: terraform/cdk/cloudflare/lambda/pulumi templates.

use std::path::Path;

use anyhow::Result;

use super::{slugify, write_file};

const NODE_TEST_COMMAND: &str = "node --test test/scaffold.test.cjs";

pub struct CloProcessor;

impl CloProcessor {
    /// Frameworks generated directly (the `_` arm is the Pulumi
    /// template). Unknown ids keep the layer-required error.
    /// (Framework tự sinh trực tiếp — id lạ vẫn fail rõ.)
    pub fn supports(framework: &str) -> bool {
        matches!(
            framework,
            "terraform"
                | "terraform-gcp"
                | "cdk"
                | "cdk-typescript"
                | "cloudflare"
                | "lambda"
                | "pulumi"
        )
    }

    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "terraform" | "terraform-gcp" => write_file(
                &target.join("main.tf"),
                "terraform {\n  required_version = \">= 1.5.0\"\n}\n\nprovider \"google\" {}\n",
            )?,
            "cdk" | "cdk-typescript" => {
                Self::write_cdk_project(target, name)?;
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
                Self::write_pulumi_project(target, name)?;
            }
        }

        Ok(())
    }

    fn write_cdk_project(target: &Path, name: &str) -> Result<()> {
        let package = serde_json::json!({
            "name": slugify(name),
            "private": true,
            "version": "0.1.0",
            "scripts": {
                "synth": "cdk synth",
                "test": NODE_TEST_COMMAND
            },
            "dependencies": {
                "aws-cdk-lib": "^2.0.0",
                "constructs": "^10.0.0"
            },
            "devDependencies": {
                "aws-cdk": "^2.0.0"
            }
        });
        write_file(
            &target.join("package.json"),
            &format!("{}\n", serde_json::to_string_pretty(&package)?),
        )?;
        write_file(
            &target.join("cdk.json"),
            "{\n  \"app\": \"node bin/app.js\"\n}\n",
        )?;
        let project_name = serde_json::to_string(&slugify(name))?;
        write_file(
            &target.join("src/app.js"),
            &format!(
                "const cdk = require('aws-cdk-lib');\n\nfunction createApp() {{\n  const app = new cdk.App();\n  const stack = new cdk.Stack(app, 'MagiCoreStack');\n  new cdk.CfnOutput(stack, 'ProjectName', {{ value: {project_name} }});\n  return {{ app, stack }};\n}}\n\nif (require.main === module) {{\n  createApp();\n}}\n\nmodule.exports = {{ createApp }};\n"
            ),
        )?;
        write_file(
            &target.join("test/scaffold.test.cjs"),
            "const test = require('node:test');\nconst { Template } = require('aws-cdk-lib/assertions');\nconst { createApp } = require('../src/app');\n\ntest('CDK app synthesizes the project output', () => {\n  const { stack } = createApp();\n  Template.fromStack(stack).hasOutput('ProjectName', {});\n});\n",
        )?;
        write_file(
            &target.join("bin/app.js"),
            "const { createApp } = require('../src/app');\ncreateApp();\n",
        )?;
        Ok(())
    }

    fn write_pulumi_project(target: &Path, name: &str) -> Result<()> {
        let project_name = slugify(name);
        let package = serde_json::json!({
            "name": project_name,
            "private": true,
            "version": "0.1.0",
            "scripts": {
                "test": NODE_TEST_COMMAND
            },
            "dependencies": {
                "@pulumi/pulumi": "^3.0.0"
            }
        });
        write_file(
            &target.join("Pulumi.yaml"),
            &format!(
                "name: {project_name}\nruntime: nodejs\nmain: index.js\ndescription: MagiCore cloud project\n"
            ),
        )?;
        write_file(
            &target.join("package.json"),
            &format!("{}\n", serde_json::to_string_pretty(&package)?),
        )?;
        write_file(
            &target.join("index.js"),
            "const pulumi = require('@pulumi/pulumi');\nexports.projectName = pulumi.getProject();\n",
        )?;
        write_file(
            &target.join("test/scaffold.test.cjs"),
            "const test = require('node:test');\nconst assert = require('node:assert/strict');\nconst fs = require('node:fs');\nconst path = require('node:path');\n\ntest('Pulumi project declares a loadable program and SDK', () => {\n  const root = path.resolve(__dirname, '..');\n  const project = fs.readFileSync(path.join(root, 'Pulumi.yaml'), 'utf8');\n  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));\n  assert.match(project, /^main: index\\.js$/m);\n  assert.ok(fs.statSync(path.join(root, 'index.js')).isFile());\n  assert.ok(manifest.dependencies['@pulumi/pulumi']);\n});\n",
        )?;
        Ok(())
    }
}
