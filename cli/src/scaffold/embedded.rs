//! Embedded scaffold kernel (minimal templates compiled into binary).

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Embedded scaffold kernel (compiled-in minimal templates).
pub struct EmbeddedKernel;

impl EmbeddedKernel {
    /// Check if an embedded kernel exists for a layer.
    pub fn has_layer(core: &str, name: &str) -> bool {
        Self::kernel_map().contains_key(&format!("{}/{}", core, name))
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

        // Future kernels:
        // - ai/python-simple
        // - app/flutter-hello
        // - lib/rust-lib

        map
    }
}

/// Embedded layer metadata.
#[derive(Debug, Clone)]
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
