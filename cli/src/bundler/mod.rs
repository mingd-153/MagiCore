use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct BundlerConfig {
    pub entry: PathBuf,
    pub output_dir: PathBuf,
    pub minify: bool,
    pub sourcemap: bool,
    pub target: String,
    pub public_path: String,
}

#[derive(Debug)]
pub struct BundleResult {
    pub size: usize,
}

pub struct Bundler {
    config: BundlerConfig,
}

struct PreparedWorkspace {
    working_dir: PathBuf,
    entry: PathBuf,
    _temp_root: Option<PathBuf>,
}

impl Bundler {
    pub fn new(config: BundlerConfig) -> Self {
        Self { config }
    }

    pub async fn bundle(&self) -> Result<BundleResult, anyhow::Error> {
        std::fs::create_dir_all(&self.config.output_dir)?;
        let prepared = prepare_workspace(&self.config.entry)?;

        let mut builder = esbuild_rs::BuildOptionsBuilder::new();
        builder.entry_points = vec![prepared.entry.to_string_lossy().to_string()];
        builder.bundle = true;
        builder.outdir = self.config.output_dir.to_string_lossy().to_string();
        builder.abs_working_dir = prepared.working_dir.to_string_lossy().to_string();
        builder.preserve_symlinks = false;
        builder.platform = esbuild_rs::Platform::Browser;
        builder.format = esbuild_rs::Format::ESModule;
        builder.main_fields = vec![
            "browser".to_string(),
            "module".to_string(),
            "main".to_string(),
        ];
        builder.resolve_extensions = vec![
            ".tsx".to_string(),
            ".ts".to_string(),
            ".jsx".to_string(),
            ".js".to_string(),
            ".css".to_string(),
            ".json".to_string(),
        ];
        builder.target = match self.config.target.as_str() {
            "es5" => esbuild_rs::Target::ES5,
            "es2015" => esbuild_rs::Target::ES2015,
            "es2016" => esbuild_rs::Target::ES2016,
            "es2017" => esbuild_rs::Target::ES2017,
            "es2018" => esbuild_rs::Target::ES2018,
            "es2019" => esbuild_rs::Target::ES2019,
            "es2020" => esbuild_rs::Target::ES2020,
            "es2021" => esbuild_rs::Target::ES2021,
            "esnext" => esbuild_rs::Target::ESNext,
            _ => esbuild_rs::Target::ES2020,
        };
        builder.minify_whitespace = self.config.minify;
        builder.minify_identifiers = self.config.minify;
        builder.minify_syntax = self.config.minify;
        builder.source_map = if self.config.sourcemap {
            esbuild_rs::SourceMap::Linked
        } else {
            esbuild_rs::SourceMap::None
        };
        builder.public_path = self.config.public_path.clone();
        builder.write = true;

        let options = builder.build();
        let result = esbuild_rs::build(options).await;

        if !result.errors.as_slice().is_empty() {
            let msgs: Vec<String> = result
                .errors
                .as_slice()
                .iter()
                .map(|e| e.to_string())
                .collect();
            return Err(anyhow::anyhow!("esbuild failed: {}", msgs.join("\n")));
        }

        let total_size: usize = result
            .output_files
            .as_slice()
            .iter()
            .map(|f| f.data.as_str().len())
            .sum();

        Ok(BundleResult { size: total_size })
    }
}

fn detect_working_dir(entry: &Path) -> Option<PathBuf> {
    let mut current = entry.parent()?;
    loop {
        if current.join("package.json").exists()
            || current.join("mg.toml").exists()
            || current.join("Cargo.toml").exists()
        {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn prepare_workspace(entry: &Path) -> Result<PreparedWorkspace, anyhow::Error> {
    let working_dir = detect_working_dir(entry)
        .unwrap_or_else(|| entry.parent().unwrap_or(Path::new(".")).to_path_buf());
    let node_modules = working_dir.join("node_modules");
    if !node_modules.exists() {
        return Ok(PreparedWorkspace {
            entry: entry.to_path_buf(),
            working_dir,
            _temp_root: None,
        });
    }

    let needs_materialized_resolver = node_modules.join(".megagate").exists();
    if !needs_materialized_resolver {
        return Ok(PreparedWorkspace {
            entry: entry.to_path_buf(),
            working_dir,
            _temp_root: None,
        });
    }

    let temp_root = working_dir.join(".megagate").join(format!(
        "tmp-build-workspace-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    mirror_project_for_build(&working_dir, &temp_root)?;
    let mirrored_node_modules = temp_root.join("node_modules");
    link_node_modules_for_build(&node_modules, &mirrored_node_modules)?;
    let relative_entry = entry.strip_prefix(&working_dir)?.to_path_buf();

    Ok(PreparedWorkspace {
        entry: temp_root.join(relative_entry),
        working_dir: temp_root.clone(),
        _temp_root: Some(temp_root),
    })
}

fn link_node_modules_for_build(source: &Path, target: &Path) -> Result<(), anyhow::Error> {
    if target.exists() {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    symlink_dir(source, target).map_err(|err| {
        anyhow::anyhow!(
            "failed to link node_modules for build from {} to {}: {}",
            source.display(),
            target.display(),
            err
        )
    })
}

#[cfg(unix)]
fn symlink_dir(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn symlink_dir(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, target)
}

fn mirror_project_for_build(source_root: &Path, target_root: &Path) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(target_root)?;
    for entry in walkdir::WalkDir::new(source_root) {
        let entry = entry?;
        let path = entry.path();
        if path == source_root {
            continue;
        }

        let relative = path.strip_prefix(source_root)?;
        if should_skip_project_path(relative) {
            continue;
        }

        let target_path = target_root.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target_path)?;
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match std::fs::hard_link(path, &target_path) {
            Ok(()) => {}
            Err(_) => {
                std::fs::copy(path, &target_path)?;
            }
        }
    }
    Ok(())
}

fn should_skip_project_path(relative: &Path) -> bool {
    let mut components = relative.components();
    let Some(first) = components.next() else {
        return false;
    };
    let first = first.as_os_str().to_string_lossy();
    matches!(
        first.as_ref(),
        "node_modules" | ".megagate" | "dist" | "build" | ".next" | "out"
    )
}

pub async fn process_assets(config: &BundlerConfig) -> Result<(), anyhow::Error> {
    let root = detect_working_dir(&config.entry).unwrap_or_else(|| {
        config
            .entry
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    materialize_index_html(&root, config)?;

    let public_dir = config.entry.parent().unwrap().join("public");
    if public_dir.exists() {
        let dst = config.output_dir.join("public");
        copy_dir_all(&public_dir, &dst)?;
    }
    Ok(())
}

fn materialize_index_html(root: &Path, config: &BundlerConfig) -> Result<(), anyhow::Error> {
    let output = config.output_dir.join("index.html");
    let bundle_name = config
        .entry
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| format!("{stem}.js"))
        .unwrap_or_else(|| "main.js".to_string());

    let source_html = root.join("index.html");
    if source_html.exists() {
        let mut html = std::fs::read_to_string(&source_html)?;
        if let Ok(relative_entry) = config.entry.strip_prefix(root) {
            let relative_entry = relative_entry.to_string_lossy().replace('\\', "/");
            for pattern in [
                format!("src=\"/{relative_entry}\""),
                format!("src=\"{relative_entry}\""),
                format!("src='/{relative_entry}'"),
                format!("src='{relative_entry}'"),
            ] {
                let replacement = pattern
                    .replace(&relative_entry, &bundle_name)
                    .replace(&format!("/{bundle_name}"), &format!("/{bundle_name}"));
                html = html.replace(&pattern, &replacement);
            }
        }
        std::fs::write(output, html)?;
        return Ok(());
    }

    let title = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("MegaGate App");
    let html = format!(
        "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"UTF-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n    <title>{title}</title>\n  </head>\n  <body>\n    <div id=\"root\"></div>\n    <script type=\"module\" src=\"/{bundle_name}\"></script>\n  </body>\n</html>\n"
    );
    std::fs::write(output, html)?;
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn prepare_workspace_links_node_modules_instead_of_mirroring_tree() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/.megagate")).unwrap();
        std::fs::create_dir_all(root.join(".megagate")).unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"demo","version":"1.0.0"}"#,
        )
        .unwrap();
        std::fs::write(root.join("src/main.tsx"), "export const ok = true;").unwrap();

        let prepared = prepare_workspace(&root.join("src/main.tsx")).unwrap();

        assert_ne!(prepared.working_dir, root);
        assert!(prepared.entry.exists());
        let linked_node_modules = prepared.working_dir.join("node_modules");
        let metadata = std::fs::symlink_metadata(&linked_node_modules).unwrap();
        assert!(metadata.file_type().is_symlink() || metadata.is_dir());
    }

    #[test]
    fn process_assets_rewrites_root_index_html_to_built_bundle() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("dist")).unwrap();
        std::fs::write(root.join("src/main.tsx"), "console.log('ok');").unwrap();
        std::fs::write(
            root.join("index.html"),
            "<!doctype html><html><body><div id=\"root\"></div><script type=\"module\" src=\"/src/main.tsx\"></script></body></html>",
        )
        .unwrap();

        let config = BundlerConfig {
            entry: root.join("src/main.tsx"),
            output_dir: root.join("dist"),
            minify: true,
            sourcemap: true,
            target: "es2020".to_string(),
            public_path: "/".to_string(),
        };

        materialize_index_html(root, &config).unwrap();

        let generated = std::fs::read_to_string(root.join("dist/index.html")).unwrap();
        assert!(generated.contains("src=\"/main.js\""));
        assert!(!generated.contains("/src/main.tsx"));
    }
}
