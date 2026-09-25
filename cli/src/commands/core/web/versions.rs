//! `web/versions.rs` — Web scaffold npm version resolution (P1 split
//! from web.rs, Tech Lead 2026-09-11). One responsibility: resolve the
//! pinned version for a scaffold package — env override first, then the
//! live npm registry, then the embedded baseline table. Never a silent
//! "latest" without a warning.
//! `web/versions.rs` — Giải quyết phiên bản npm cho web scaffold (tách
//! từ web.rs theo P1). Một trách nhiệm: resolve version ghim cho package
//! scaffold — env override trước, rồi npm registry sống, rồi bảng
//! baseline nhúng; không bao giờ rơi "latest" âm thầm không cảnh báo.

use std::collections::HashMap;

use anyhow::Result;

/// Default registry — overridable via config elsewhere; this module
/// only READS from it (RULE §12: named constant, not inline literal).
/// Registry mặc định — module chỉ ĐỌC (RULE §12: hằng số có tên).
pub(crate) const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";

/// Env override list ("pkg@^1,pkg2@^2") for hermetic CI scaffolds.
/// Danh sách env ghi đè version cho scaffold CI hermetic.
const SCAFFOLD_VERSION_OVERRIDES_ENV: &str = "MAGICORE_WEB_SCAFFOLD_VERSION_OVERRIDES";

/// Version baseline — trước đây ở templates/web/versions/scaffold-baseline.toml;
/// registry-first (templates/ xóa khỏi repo) → giữ thẳng trong code.
const SCAFFOLD_BASELINE_VERSIONS_TOML: &str = r#"[versions]
vue = "^3.5.39"
react = "^19.2.7"
react-dom = "^19.2.7"
vite = "^8.1.4"
"@vitejs/plugin-react" = "^6.0.3"
"@vitejs/plugin-vue" = "^6.0.7"
solid-js = "^1.9.14"
vite-plugin-solid = "^2.11.12"
typescript = "^5.9.2"
"@types/react" = "^19.2.17"
"@types/react-dom" = "^19.2.3"
"@types/node" = "^26.1.1"
tailwindcss = "^4.3.2"
"@sveltejs/kit" = "^2.69.2"
"@sveltejs/vite-plugin-svelte" = "^7.2.0"
"@sveltejs/adapter-auto" = "^7.0.1"
svelte = "^5.56.4"
next = "^16.2.10"
nuxt = "^4.4.8"
"@angular/core" = "^22.0.6"
"@angular/platform-browser" = "^22.0.6"
"@angular/platform-browser-dynamic" = "^22.0.6"
"@angular/router" = "^22.0.6"
"@angular/compiler" = "^22.0.6"
"@angular/common" = "^22.0.6"
rxjs = "^7.8.2"
"zone.js" = "^0.16.2"
tslib = "^2.8.1"
"@angular/cli" = "^22.0.6"
"@angular/compiler-cli" = "^22.0.6"
"@angular-devkit/build-angular" = "^22.0.6"
"@builder.io/qwik" = "^1.20.0"
"@builder.io/qwik-city" = "^1.20.0"
astro = "^7.0.7"
express = "^5.2.1"
"@types/express" = "^5.0.6"
hono = "^4.12.30"
"@hono/node-server" = "^1.19.6"
"@nestjs/core" = "^11.1.28"
"@nestjs/common" = "^11.1.28"
"@nestjs/platform-express" = "^11.1.28"
reflect-metadata = "^0.2.2"
zod = "^4.4.3"
"@trpc/server" = "^11.18.0"
fastify = "^5.10.0"
tsx = "^4.23.0"
"@prisma/client" = "^6.6.0"
prisma = "^6.6.0"
vitest = "^3.2.0"
eslint = "^9.28.0"
eslint-config-next = "^16.2.10"
prettier = "^3.6.0"
"@tailwindcss/postcss" = "^4.3.2"
pg = "^8.15.0"
zustand = "^5.0.3"
"@tanstack/react-query" = "^5.62.0"
next-auth = "^5.0.0"
"@playwright/test" = "^1.52.0"
husky = "^9.2.0"
lint-staged = "^15.5.0"
"@biomejs/biome" = "^1.9.0"
clsx = "^2.1.0"
tailwind-merge = "^3.2.0"
class-variance-authority = "^0.7.0"
sass = "^1.83.0"
unocss = "^65.5.0"
daisyui = "^4.12.0"
"@reduxjs/toolkit" = "^2.6.0"
react-redux = "^9.2.0"
jest = "^29.7.0"
"@testing-library/react" = "^16.0.0"
"@testing-library/jest-dom" = "^6.6.0"
jest-environment-jsdom = "^29.7.0"
cypress = "^14.2.0"
drizzle-orm = "^0.38.0"
drizzle-kit = "^0.30.0"
"@clerk/nextjs" = "^6.12.0"
styled-components = "^6.1.0"
"@types/styled-components" = "^5.1.0"
"@commitlint/cli" = "^19.5.0"
"@commitlint/config-conventional" = "^19.5.0"
"@apollo/server" = "^4.11.0"
"@as-integrations/next" = "^3.2.0"
"@trpc/client" = "^11.0.0"
"@trpc/next" = "^11.0.0"
"@grpc/grpc-js" = "^1.11.0"
"@grpc/proto-loader" = "^0.7.0"
lucia = "^3.2.0"
"@lucia-auth/adapter-drizzle" = "^1.1.0"
jose = "^5.7.0"
dotenv-cli = "^7.4.0"
next-i18next = "^15.3.0"
next-pwa = "^5.6.0"
"@storybook/nextjs" = "^8.2.0"
"@storybook/react" = "^8.2.0"
"@sentry/nextjs" = "^8.21.0"
"@vercel/analytics" = "^1.3.0"
"@railway/cli" = "^4.2.0"
flyctl = "^0.2.0"
"#;

/// TOML shape of the baseline table.
/// Kết cấu TOML của bảng baseline.
#[derive(serde::Deserialize)]
struct ScaffoldBaselineVersions {
    versions: HashMap<String, String>,
}

/// Resolve the scaffold version for a package: env override → live
/// npm registry → embedded baseline → warn + "latest".
/// Resolve version scaffold cho package: env ghi đè → npm registry
/// sống → baseline nhúng → cảnh báo + "latest".
pub(crate) async fn fetch_npm_latest_version(package: &str) -> Result<String> {
    if let Some(version) = scaffold_version_override(package) {
        return Ok(version);
    }
    match fetch_npm_latest_version_from_registry(DEFAULT_NPM_REGISTRY, package).await {
        Ok(version) => Ok(version),
        Err(error) => {
            if let Some(version) = scaffold_baseline_version(package) {
                return Ok(version.to_string());
            }
            eprintln!(
                "warning: could not resolve version for '{package}' ({}); using 'latest'",
                error
            );
            Ok("latest".to_string())
        }
    }
}

/// Shared HTTP client (pooled — one client for every version lookup).
/// HTTP client dùng chung (pool — một client cho mọi lần tra version).
fn global_cli_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(50)
            .pool_idle_timeout(std::time::Duration::from_secs(120))
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(format!("MagiCore/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client")
    })
}

async fn fetch_npm_latest_version_from_registry(
    registry_url: &str,
    package: &str,
) -> Result<String> {
    let url = format!("{}/{package}/latest", registry_url.trim_end_matches('/'));
    let resp = global_cli_http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::error::network_error_fetching(package, &e))?;
    if !resp.status().is_success() {
        return Err(crate::error::npm_registry_status(package, &resp.status()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| crate::error::bad_npm_response(package, &e))?;
    parse_latest_version_response(package, &body)
}

fn parse_latest_version_response(package: &str, body: &serde_json::Value) -> Result<String> {
    body["version"]
        .as_str()
        .map(|s| format!("^{}", s))
        .ok_or_else(|| crate::error::no_version_field(package))
}

fn scaffold_version_override(package: &str) -> Option<String> {
    std::env::var(SCAFFOLD_VERSION_OVERRIDES_ENV)
        .ok()
        .and_then(|raw| {
            raw.split(',')
                .filter_map(|entry| entry.trim().split_once('='))
                .find_map(|(name, version)| {
                    (name.trim() == package && !version.trim().is_empty())
                        .then(|| version.trim().to_string())
                })
        })
}

/// The embedded baseline table (parsed once, reused forever).
/// Bảng baseline nhúng (parse một lần, dùng mãi).
pub(crate) fn scaffold_baseline_versions() -> &'static HashMap<String, String> {
    static VERSIONS: std::sync::OnceLock<HashMap<String, String>> = std::sync::OnceLock::new();
    VERSIONS.get_or_init(|| {
        toml::from_str::<ScaffoldBaselineVersions>(SCAFFOLD_BASELINE_VERSIONS_TOML)
            .expect("templates/web/versions/scaffold-baseline.toml must be valid")
            .versions
    })
}

/// Baseline lookup for one package (None = not in the table).
/// Tra baseline một package (None = không có trong bảng).
pub(crate) fn scaffold_baseline_version(package: &str) -> Option<&'static str> {
    scaffold_baseline_versions()
        .get(package)
        .map(String::as_str)
}
