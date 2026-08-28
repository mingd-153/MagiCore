# `assets/` — Brand Assets & Board Registry

## Logo

| File | Usage |
|---|---|
| `logo.svg` | Full MagiCore logo (use in README, website, social) |
| `logo-in.svg` | Inline / compact variant (use in badges, IDE extensions) |
| `favicon.ico` | Browser favicon for the registry web UI |

## `boards/` — IoT Board Registry

Predefined board configurations for the `iot` ecosystem adapter.

Contains JSON/TOML board definitions for:
- ESP32 / ESP8266 (PlatformIO)
- Zephyr RTOS targets
- STM32 family
- Arduino-compatible boards
- Nordic nRF52840

These are used by `mgc create-iot --board <name>` to generate correct project scaffolding and toolchain configuration.
