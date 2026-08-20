//! Library scaffold: ts/python/rust templates.

use std::path::Path;

use anyhow::Result;

use super::{slugify, write_file};

pub struct LibProcessor;

impl LibProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "ts" | "typescript" => {
                write_file(
                    &target.join("package.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"devDependencies\": {{\"typescript\": \"^5\"}}\n}}\n",
                        slugify(name)
                    ),
                )?;
                write_file(
                    &target.join("tsconfig.json"),
                    "{\n  \"compilerOptions\": {\n    \"target\": \"ES2022\",\n    \"module\": \"ESNext\"\n  }\n}\n",
                )?;
                write_file(
                    &target.join("src").join("index.ts"),
                    "export function hello(): string {\n    return 'hello from MegaGate';\n}\n",
                )?;
            }
            "python" => {
                let package = slugify(name).replace('-', "_");
                write_file(
                    &target.join("pyproject.toml"),
                    &format!(
                        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\n",
                        slugify(name)
                    ),
                )?;
                write_file(
                    &target.join("src").join(&package).join("__init__.py"),
                    "__all__ = []\n",
                )?;
            }
            _ => {
                write_file(
                    &target.join("Cargo.toml"),
                    &format!(
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                        slugify(name)
                    ),
                )?;
                write_file(
                    &target.join("src").join("lib.rs"),
                    "pub fn hello() -> &'static str {\n    \"hello from MegaGate\"\n}\n",
                )?;
            }
        }

        Ok(())
    }
}
