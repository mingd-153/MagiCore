//! IoT scaffold: platformio/zephyr/esp32-rust templates.

use std::path::Path;

use anyhow::Result;

use super::{slugify, write_file};

pub struct IotProcessor;

impl IotProcessor {
    /// Frameworks this processor generates directly (no registry layer
    /// needed). Anything else keeps the layer-required error instead of
    /// receiving a mislabeled fallback (e.g. board variant `esp32dev`
    /// must NOT get a generic Cargo project).
    /// (Framework processor tự sinh — ngoài danh sách vẫn fail rõ.)
    pub fn supports(framework: &str) -> bool {
        matches!(
            framework,
            "platformio" | "firmware" | "zephyr" | "zephyr-arm" | "esp32-rust"
        )
    }

    pub fn files(target: &Path, name: &str, framework: &str, board: &str) -> Result<()> {
        match framework {
            "platformio" | "firmware" => {
                write_file(
                    &target.join("platformio.ini"),
                    &format!(
                        "[env:{board}]\nplatform = espressif32\nboard = {board}\nframework = arduino\n"
                    ),
                )?;
                write_file(
                    &target.join("src").join("main.cpp"),
                    "#include <Arduino.h>\n\nvoid setup() {\n}\n\nvoid loop() {\n}\n",
                )?;
            }
            "zephyr" | "zephyr-arm" => write_file(
                &target.join("west.yml"),
                "manifest:\n  version: 0.13\n  projects: []\n",
            )?,
            "esp32-rust" => {
                // esp-hal on crates.io (verified resolvable: esp-hal 1.2.2
                // exposes matching esp32/esp32c3/esp32s3 features. Building
                // needs the Espressif toolchain (not provisioned here).
                // (esp-hal 1.2.2 có feature khớp ba board; build cần toolchain Espressif.)
                let feature = match board {
                    "esp32" => "esp32",
                    "esp32c3" => "esp32c3",
                    "esp32s3" => "esp32s3",
                    _ => anyhow::bail!("No verified esp-hal feature for board '{board}'"),
                };
                write_file(
                    &target.join("Cargo.toml"),
                    &format!(
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nesp-hal = {{ version = \"1.2.2\", features = [\"{feature}\"] }}\n",
                        slugify(name),
                    ),
                )?;
                write_file(
                    &target.join("src").join("main.rs"),
                    "#![no_std]\n#![no_main]\n\nuse esp_hal::main;\n\n#[main]\nfn main() -> ! {\n    loop {}\n}\n",
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
                    &target.join("src").join("main.rs"),
                    "#![no_std]\n#![no_main]\n\n#[no_mangle]\npub extern \"C\" fn main() -> ! {\n    loop {}\n}\n",
                )?;
            }
        }

        Ok(())
    }
}
