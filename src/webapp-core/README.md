# Webapp Core

This directory contains the core backend and shared client‑side code for the MegaGate web application.

- **api/** – endpoint definitions and routing.
- **service/** – business logic, orchestrating repository calls.
- **domain/** – domain entities, enums, value objects.
- **repository/** – data access interfaces and implementations.
- **config/** – configuration files and environment constants.
- **util/** – utility functions, logging, validation helpers.
- **tests/** – unit and integration tests for the core modules.
- **app/** – client‑side application code for different platforms (Swift, Kotlin, Dart, TypeScript).
- **web/static/** – static web assets (HTML, CSS, images, compiled JavaScript).
- **shared/** – shared resources used across multiple platforms.

The module entry point is `src/webapp-core/mod.rs` which re‑exports the sub‑modules for Rust code. For JavaScript/TypeScript usage, an `index.ts` could be added in the future.
