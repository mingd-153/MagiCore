//! IoT scaffold: platformio/zephyr/esp32-rust templates.

use std::path::Path;

use anyhow::Result;

use super::{slugify, write_file};

pub struct IotProcessor;

impl IotProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "platformio" | "firmware" => {
                write_file(
                    &target.join("platformio.ini"),
                    "[env:esp32dev]\nplatform = espressif32\nboard = esp32dev\nframework = arduino\n",
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
