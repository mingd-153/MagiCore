//! Central version manifest for all template packages.
//!
//! All template package versions are declared here in one place.
//! To upgrade a package, change its constant here — every template
//! that uses it picks up the new version automatically.
//!
//! Convention: constant name = npm package name in SCREAMING_SNAKE_CASE
//! (dots become underscores).

// --- Frameworks ---
pub const REACT: &str = "^19.0.0";
pub const REACT_DOM: &str = "^19.0.0";
pub const REACT_ROUTER: &str = "^7.0.0";

// --- State management ---
pub const ZUSTAND: &str = "^5.0.0";

// --- Build tools ---
pub const VITE: &str = "^6.0.0";
pub const VITE_PLUGIN_REACT: &str = "^4.0.0";

// --- CSS / Styling ---
pub const TAILWINDCSS: &str = "^4.0.0";
pub const TAILWINDCSS_VITE: &str = "^4.0.0";
pub const POSTCSS: &str = "^8.4.0";
pub const AUTOPREFIXER: &str = "^10.4.0";
pub const SASS: &str = "^1.80.0";
pub const BOOTSTRAP: &str = "^5.3.3";

// --- TypeScript toolchain ---
pub const TYPESCRIPT: &str = "^5.7.0";
pub const TYPES_NODE: &str = "^22.0.0";
pub const TYPES_REACT: &str = "^19.0.0";
pub const TYPES_REACT_DOM: &str = "^19.0.0";

// --- Linting / Formatting ---
pub const ESLINT: &str = "^9.0.0";
pub const ESLINT_JS: &str = "^9.0.0";
pub const ESLINT_PLUGIN_REACT: &str = "^7.37.0";
pub const ESLINT_PLUGIN_REACT_HOOKS: &str = "^5.0.0";
pub const ESLINT_PLUGIN_REACT_REFRESH: &str = "^0.5.0";
pub const GLOBALS: &str = "^16.0.0";
pub const TYPESCRIPT_ESLINT: &str = "^8.0.0";
pub const PRETTIER: &str = "^3.0.0";
