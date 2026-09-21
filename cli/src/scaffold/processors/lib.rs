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
                        "{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"test\": \"tsc --noEmit\"\n  }},\n  \"devDependencies\": {{\"typescript\": \"^5\"}}\n}}\n",
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
                // The scaffold ships its OWN test (P0 finding,
                // 2026-09-12): `mgc test` auto-detects pytest via
                // pyproject.toml — without a test the honest lifecycle
                // fails with pytest exit 5. The test imports the
                // package through the installed name (pip install -e
                // makes it importable); no PYTHONPATH hacks.
                // Template TỰ mang test (P0 finding, 2026-09-12):
                // `mgc test` auto-detect pytest qua pyproject.toml —
                // thiếu test thì lifecycle trung thực fail với pytest
                // exit 5. Test import package qua TÊN đã cài (pip
                // install -e cho import được); không cần PYTHONPATH.
                write_file(
                    &target.join("tests").join("test_package.py"),
                    &format!(
                        "\"\"\"Scaffold smoke test: the package imports.\"\"\"\n\nimport importlib\n\n\ndef test_package_imports() -> None:\n    module = importlib.import_module(\"{package}\")\n    assert module.__all__ == []\n"
                    ),
                )?;
                // src-layout needs src/ on sys.path — a local conftest.py
                // does it explicitly (no `pip install -e`, no PYTHONPATH
                // hacks), so `mgc test` passes on the native lane too.
                // (conftest.py đưa src/ vào sys.path cho lane native.)
                write_file(
                    &target.join("tests").join("conftest.py"),
                    "import sys\nfrom pathlib import Path\n\nsys.path.insert(0, str(Path(__file__).resolve().parent.parent / \"src\"))\n",
                )?;
            }
            "go" => {
                // Native lane needs a module line (detect_language +
                // go.mod writer); ship a compiling package with its own
                // test so `go test` proves the scaffold (P0 convention).
                write_file(
                    &target.join("go.mod"),
                    &format!("module {}\n\ngo 1.21\n", slugify(name)),
                )?;
                let package = slugify(name).replace('-', "_");
                write_file(
                    &target.join("hello.go"),
                    &format!(
                        "package {package}\n\n// Hello proves the scaffold compiles.\nfunc Hello() string {{\n\treturn \"hello from MagiCore\"\n}}\n",
                    ),
                )?;
                write_file(
                    &target.join("hello_test.go"),
                    &format!(
                        "package {package}\n\nimport \"testing\"\n\nfunc TestHello(t *testing.T) {{\n\tif Hello() != \"hello from MagiCore\" {{\n\t\tt.Fatal(\"scaffold greeting mismatch\")\n\t}}\n}}\n",
                    ),
                )?;
            }
            "dotnet" => {
                // SDK-style csproj so NuGet audit/add lanes detect the
                // project (dotnet SDK itself stays external).
                write_file(
                    &target.join(format!("{}.csproj", slugify(name))),
                    "<Project Sdk=\"Microsoft.NET.Sdk\">\n\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n    <Nullable>enable</Nullable>\n  </PropertyGroup>\n\n</Project>\n",
                )?;
            }
            "java" => {
                // Minimal pom.xml so the Maven audit/resolve lanes detect
                // the project (Maven plugins stay external).
                write_file(
                    &target.join("pom.xml"),
                    &format!(
                        "<project>\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>com.magicore</groupId>\n  <artifactId>{}</artifactId>\n  <version>0.1.0</version>\n  <properties>\n    <maven.compiler.source>17</maven.compiler.source>\n    <maven.compiler.target>17</maven.compiler.target>\n  </properties>\n</project>\n",
                        slugify(name)
                    ),
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
                // cargo test with zero #[test]s exits 0 with "0 tests
                // run" — a pass with zero evidence. Ship a real
                // assertion so `mgc test` (cargo test) proves the
                // scaffold compiles AND behaves.
                // cargo test với 0 #[test] exit 0 với "0 tests run" —
                // pass không bằng chứng. Mang assertion thật để `mgc
                // test` (cargo test) chứng minh template vừa compile
                // vừa chạy đúng.
                write_file(
                    &target.join("src").join("lib.rs"),
                    "pub fn hello() -> &'static str {\n    \"hello from MagiCore\"\n}\n\n#[cfg(test)]\nmod scaffold_tests {\n    #[test]\n    fn hello_returns_scaffold_greeting() {\n        assert_eq!(super::hello(), \"hello from MagiCore\");\n    }\n}\n",
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

#[cfg(test)]
mod fidelity_tests {
    use super::LibProcessor;

    /// `create-lib go` must scaffold Go files (not the rust fallback) —
    /// go.mod with a module line so detect_language + native add work.
    #[test]
    fn go_scaffold_emits_go_mod_not_cargo() {
        let temp = tempfile::tempdir().expect("tempdir");
        LibProcessor::files(temp.path(), "demo-lib", "go").expect("go scaffold");
        assert!(
            !temp.path().join("Cargo.toml").exists(),
            "go scaffold must not contain Cargo.toml"
        );
        let gomod = std::fs::read_to_string(temp.path().join("go.mod")).expect("go.mod exists");
        assert!(gomod.contains("module demo-lib"), "module line: {gomod}");
    }

    /// `create-lib dotnet` must scaffold a .csproj (not the rust fallback)
    /// so NuGet audit/add lanes detect the project.
    #[test]
    fn dotnet_scaffold_emits_csproj_not_cargo() {
        let temp = tempfile::tempdir().expect("tempdir");
        LibProcessor::files(temp.path(), "demo-lib", "dotnet").expect("dotnet scaffold");
        assert!(
            !temp.path().join("Cargo.toml").exists(),
            "dotnet scaffold must not contain Cargo.toml"
        );
        assert!(
            temp.path().join("demo-lib.csproj").exists(),
            "csproj must exist"
        );
    }
}

#[cfg(test)]
mod fidelity_java_test {
    use super::LibProcessor;

    /// `create-lib java` must scaffold a pom.xml (not the rust fallback).
    #[test]
    fn java_scaffold_emits_pom_not_cargo() {
        let temp = tempfile::tempdir().expect("tempdir");
        LibProcessor::files(temp.path(), "demo-lib", "java").expect("java scaffold");
        assert!(
            !temp.path().join("Cargo.toml").exists(),
            "java scaffold must not contain Cargo.toml"
        );
        let pom = std::fs::read_to_string(temp.path().join("pom.xml")).expect("pom exists");
        assert!(
            pom.contains("<artifactId>demo-lib</artifactId>"),
            "coords: {pom}"
        );
    }
}
