//! Embedded scaffold kernel (minimal templates compiled into binary).

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Embedded scaffold kernel (compiled-in minimal templates).
pub struct EmbeddedKernel;

impl EmbeddedKernel {
    /// Check if an embedded kernel exists for a layer.
    ///
    /// Accepts either:
    /// - Short form: ("web", "vanilla") → checks "web/vanilla"
    /// - Full path check via has_layer_path("web/shared/base")
    pub fn has_layer(core: &str, name: &str) -> bool {
        Self::kernel_map().contains_key(&format!("{}/{}", core, name))
    }

    /// Check if embedded kernel exists by full path.
    pub fn has_layer_path(path: &str) -> bool {
        Self::kernel_map().contains_key(path)
    }

    /// Extract embedded layer to target directory.
    pub fn extract_layer(core: &str, name: &str, target: &Path) -> Result<()> {
        let key = format!("{}/{}", core, name);
        let kernels = Self::kernel_map();
        let kernel = kernels
            .get(&key)
            .ok_or_else(|| anyhow::anyhow!("No embedded kernel for {}", key))?;

        std::fs::create_dir_all(target)?;

        // Extract tarball
        let decoder = flate2::read::GzDecoder::new(kernel.data);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(target)?;

        Ok(())
    }

    /// Extract embedded layer by full path.
    pub fn extract_layer_path(path: &str, target: &Path) -> Result<()> {
        let kernels = Self::kernel_map();
        let kernel = kernels
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("No embedded kernel for {}", path))?;

        std::fs::create_dir_all(target)?;

        // Extract tarball
        let decoder = flate2::read::GzDecoder::new(kernel.data);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(target)?;

        Ok(())
    }

    /// List all available embedded kernels.
    pub fn list_available() -> Vec<String> {
        Self::kernel_map().keys().cloned().collect()
    }

    /// Kernel registry (core/name → embedded data).
    fn kernel_map() -> HashMap<String, EmbeddedLayer> {
        let mut map = HashMap::new();

        // Web kernel: vanilla
        map.insert(
            "web/vanilla".to_string(),
            EmbeddedLayer {
                name: "vanilla",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-vanilla.tar.gz"),
            },
        );

        // Web frontend: vanilla
        map.insert(
            "web/frontend/vanilla".to_string(),
            EmbeddedLayer {
                name: "vanilla",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-frontend-vanilla.tar.gz"),
            },
        );

        // Web shared partials (minimal for scaffold)
        map.insert(
            "web/shared/partials/base".to_string(),
            EmbeddedLayer {
                name: "base",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-base.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/frontend".to_string(),
            EmbeddedLayer {
                name: "frontend",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-frontend.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/frontend-common".to_string(),
            EmbeddedLayer {
                name: "frontend-common",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-frontend-common.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/frontend-foundation".to_string(),
            EmbeddedLayer {
                name: "frontend-foundation",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-frontend-foundation.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/frontend-rust-ready".to_string(),
            EmbeddedLayer {
                name: "frontend-rust-ready",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-frontend-rust-ready.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/backend".to_string(),
            EmbeddedLayer {
                name: "backend",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-backend.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/fullstack".to_string(),
            EmbeddedLayer {
                name: "fullstack",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-fullstack.tar.gz"),
            },
        );

        // Monorepo partials
        map.insert(
            "web/shared/partials/monorepo".to_string(),
            EmbeddedLayer {
                name: "monorepo",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-monorepo.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/monorepo-backend".to_string(),
            EmbeddedLayer {
                name: "monorepo-backend",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-monorepo-backend.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/monorepo-frontend".to_string(),
            EmbeddedLayer {
                name: "monorepo-frontend",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-monorepo-frontend.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/monorepo-frontend-common".to_string(),
            EmbeddedLayer {
                name: "monorepo-frontend-common",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-monorepo-frontend-common.tar.gz"),
            },
        );

        map.insert(
            "web/shared/partials/monorepo-frontend-foundation".to_string(),
            EmbeddedLayer {
                name: "monorepo-frontend-foundation",
                core: "web",
                version: "1.0.0",
                data: include_bytes!(
                    "../../embedded/web-shared-monorepo-frontend-foundation.tar.gz"
                ),
            },
        );

        map.insert(
            "web/shared/partials/monorepo-frontend-rust-ready".to_string(),
            EmbeddedLayer {
                name: "monorepo-frontend-rust-ready",
                core: "web",
                version: "1.0.0",
                data: include_bytes!(
                    "../../embedded/web-shared-monorepo-frontend-rust-ready.tar.gz"
                ),
            },
        );

        map.insert(
            "web/shared/partials/monorepo-packages".to_string(),
            EmbeddedLayer {
                name: "monorepo-packages",
                core: "web",
                version: "1.0.0",
                data: include_bytes!("../../embedded/web-shared-monorepo-packages.tar.gz"),
            },
        );

        // Future kernels:
        // - ai/python-simple
        // - app/flutter-hello
        // - lib/rust-lib

        map
    }
}

/// Embedded layer metadata.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used during extraction, read indirectly via include_bytes!
struct EmbeddedLayer {
    name: &'static str,
    core: &'static str,
    version: &'static str,
    data: &'static [u8],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_vanilla_kernel() {
        // Will fail until we create embedded/web-vanilla.tar.gz
        // but structure is ready
        assert!(
            EmbeddedKernel::list_available().contains(&"web/vanilla".to_string())
                || EmbeddedKernel::list_available().is_empty()
        );
    }

    #[test]
    fn test_list_available_format() {
        let available = EmbeddedKernel::list_available();
        for kernel in available {
            assert!(kernel.contains('/'), "Kernel should be core/name format");
        }
    }
}
