//! Central version manifest for all template packages.
//!
//! Each version is fetched lazily from the npm registry on first access.
//! Falls back to hardcoded defaults if the network is unavailable.
//!
//! Convention: function name = npm package name in SCREAMING_SNAKE_CASE
//! (dots become underscores).

use std::sync::LazyLock;

fn npm_version(pkg: &str, default: &'static str) -> String {
    let url = format!("https://registry.npmjs.org/{pkg}/latest");
    match ureq::get(&url).call() {
        Ok(resp) => match resp.into_json::<serde_json::Value>() {
            Ok(json) => json
                .get("version")
                .and_then(|v| v.as_str())
                .map(|v| format!("^{v}"))
                .unwrap_or_else(|| default.to_string()),
            Err(_) => default.to_string(),
        },
        Err(_) => default.to_string(),
    }
}

macro_rules! version {
    ($name:ident, $pkg:expr, $default:expr) => {
        #[allow(non_snake_case)]
        pub fn $name() -> &'static str {
            static V: LazyLock<String> = LazyLock::new(|| npm_version($pkg, $default));
            V.as_str()
        }
    };
}

// --- Frameworks ---
version!(REACT, "react", "^19.2.7");
version!(REACT_DOM, "react-dom", "^19.0.0");
version!(REACT_ROUTER, "react-router", "^7.0.0");

// --- Vue ---
version!(VUE, "vue", "^3.5.13");
version!(VUE_ROUTER, "vue-router", "^4.5.1");
version!(PINIA, "pinia", "^2.3.1");
version!(VITE_PLUGIN_VUE, "@vitejs/plugin-vue", "^5.2.4");
version!(VUE_TSC, "vue-tsc", "^2.2.2");
version!(ESLINT_PLUGIN_VUE, "eslint-plugin-vue", "^9.33.0");
version!(VUE_ESLINT_PARSER, "vue-eslint-parser", "^9.4.3");

// --- Astro ---
version!(ASTRO, "astro", "^5.9.1");
version!(ASTRO_CHECK, "@astrojs/check", "^0.10.1");

// --- Nuxt ---
version!(NUXT, "nuxt", "^3.16.3");
version!(NUXT_ESLINT, "@nuxt/eslint", "^1.3.1");

// --- State management ---
version!(ZUSTAND, "zustand", "^5.0.14");

// --- Build tools ---
version!(VITE, "vite", "^8.1.3");
version!(VITE_PLUGIN_REACT, "@vitejs/plugin-react", "^4.0.0");

// --- Frameworks (Next.js) ---
version!(NEXT, "next", "^16.2.10");
version!(NEXT_ESLINT_PLUGIN, "@next/eslint-plugin-next", "^15.0.0");

// --- CSS / Styling ---
version!(CLSX, "clsx", "^2.1.0");
version!(TAILWIND_MERGE, "tailwind-merge", "^3.0.0");
version!(TAILWINDCSS, "tailwindcss", "^4.3.2");
version!(TAILWINDCSS_VITE, "@tailwindcss/vite", "^4.0.0");
version!(TAILWINDCSS_POSTCSS, "@tailwindcss/postcss", "^4.0.0");
version!(POSTCSS, "postcss", "^8.4.0");
version!(AUTOPREFIXER, "autoprefixer", "^10.4.0");
version!(SASS, "sass", "^1.80.0");
version!(BOOTSTRAP, "bootstrap", "^5.3.3");
version!(PRETTIER_PLUGIN_TAILWINDCSS, "prettier-plugin-tailwindcss", "^0.6.0");

// --- TypeScript toolchain ---
version!(TYPESCRIPT, "typescript", "^5.7.0");
version!(TYPES_NODE, "@types/node", "^22.0.0");
version!(TYPES_REACT, "@types/react", "^19.0.0");
version!(TYPES_REACT_DOM, "@types/react-dom", "^19.0.0");

// --- Fastify ---
version!(FASTIFY, "fastify", "^5.3.1");
version!(FASTIFY_CORS, "@fastify/cors", "^11.0.1");
version!(FASTIFY_HELMET, "@fastify/helmet", "^13.0.1");
version!(ZOD, "zod", "^3.24.4");
version!(TSX, "tsx", "^4.19.4");

// --- Linting / Formatting ---
version!(ESLINT, "eslint", "^9.0.0");
version!(ESLINT_JS, "@eslint/js", "^9.0.0");
version!(ESLINT_PLUGIN_REACT, "eslint-plugin-react", "^7.37.0");
version!(ESLINT_PLUGIN_REACT_HOOKS, "eslint-plugin-react-hooks", "^5.0.0");
version!(ESLINT_PLUGIN_REACT_REFRESH, "eslint-plugin-react-refresh", "^0.5.0");
version!(GLOBALS, "globals", "^16.0.0");
version!(TYPESCRIPT_ESLINT, "typescript-eslint", "^8.0.0");
version!(PRETTIER, "prettier", "^3.0.0");

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fetch_cached() {
        // First call may hit network, second returns cached
        let v1 = REACT();
        let v2 = REACT();
        assert!(!v1.is_empty());
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_all_have_values() {
        // Sanity: all version functions return non-empty
        assert!(!REACT().is_empty());
        assert!(!NEXT().is_empty());
        assert!(!TYPESCRIPT().is_empty());
        assert!(!CLSX().is_empty());
        assert!(!TAILWIND_MERGE().is_empty());
        assert!(!TAILWINDCSS().is_empty());
        assert!(!VITE().is_empty());
        assert!(!PRETTIER().is_empty());
        assert!(!ESLINT().is_empty());
    }

    #[test]
    fn test_semver_or_caret_format() {
        let v = REACT();
        // Must start with ^ or be a valid semver
        assert!(v.starts_with('^') || v.chars().next().map_or(false, |c| c.is_ascii_digit()));
        assert!(!v.contains(' '));
    }
}
