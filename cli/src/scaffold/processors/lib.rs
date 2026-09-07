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
                    "{\n  \"compilerOptions\": {\n    \"target\": \"ES2022\",\n    \"module\": \"ESNext\",\n    \"declaration\": true,\n    \"outDir\": \"dist\",\n    \"rootDir\": \"src\",\n    \"strict\": true\n  },\n  \"include\": [\"src/**/*.ts\"]\n}\n",
                )?;
                write_file(
                    &target.join("src").join("index.ts"),
                    "export function hello(): string {\n    return 'hello from MagiCore';\n}\n",
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
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
                        slugify(name)
                    ),
                )?;
                write_file(
                    &target.join("src").join("lib.rs"),
                    "pub fn hello() -> &'static str {\n    \"hello from MagiCore\"\n}\n",
                )?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LibProcessor;

    #[test]
    fn rust_scaffold_is_standalone_inside_parent_workspace() {
        let temp = tempfile::tempdir().expect("tempdir");
        LibProcessor::files(temp.path(), "demo-lib", "rust").expect("rust scaffold");
        let manifest = std::fs::read_to_string(temp.path().join("Cargo.toml")).expect("manifest");
        assert!(manifest.contains("edition = \"2024\""));
        assert!(manifest.contains("\n[workspace]\n"));
    }

    #[test]
    fn typescript_scaffold_emits_publishable_dist() {
        let temp = tempfile::tempdir().expect("tempdir");
        LibProcessor::files(temp.path(), "demo-lib", "typescript").expect("ts scaffold");
        let config = std::fs::read_to_string(temp.path().join("tsconfig.json")).expect("config");
        assert!(config.contains("\"outDir\": \"dist\""));
        assert!(config.contains("\"declaration\": true"));
        assert!(config.contains("\"strict\": true"));
    }
}
