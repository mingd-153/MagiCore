//! Hardware scaffold: optimizer/bench package templates.
//! (Scaffold phần cứng: template package optimizer/bench.)
//!
//! These are marker + config files only — the real work happens in
//! `mgc optimizer` (hardware-profiled config generation) and `mgc bench`
//! (timed install). No toolchain spawn here.
//! (Chỉ file marker + config — việc thật nằm ở `mgc optimizer` và
//! `mgc bench`. Không spawn toolchain ở đây.)

use std::path::Path;

use anyhow::Result;

use super::{slugify, write_file};

pub struct HardwareProcessor;

impl HardwareProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "optimizer" => {
                write_file(
                    &target.join("optimizer.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"kind\": \"optimizer\",\n  \"description\": \"MagiCore hardware optimizer package — run `mgc optimizer` here to generate hardware-profiled configs into .mgc-optimizer/\"\n}}\n",
                        slugify(name)
                    ),
                )?;
            }
            _ => {
                // "bench" and any future hardware package share the
                // benchmark-harness marker (unknown names are rejected
                // earlier by create-hardware/add-hardware validation).
                // (bench và package phần cứng khác dùng chung marker.)
                write_file(
                    &target.join("bench.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"kind\": \"bench\",\n  \"description\": \"MagiCore hardware benchmark package — run `mgc bench` here to time installs\"\n}}\n",
                        slugify(name)
                    ),
                )?;
            }
        }
        Ok(())
    }
}
