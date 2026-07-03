use std::path::{Path, PathBuf};

use heck::ToUpperCamelCase;

use crate::engine::{ProjectCreated, ScaffoldContext, ScaffoldEngine};
use crate::error::ScaffoldError;
use crate::templates::Template;
use crate::validate::NameValidator;

pub struct VanillaCodegen;

struct Ctx {
    name: String,
    version: String,
    has_vite: bool,
    has_ts: bool,
    has_tailwind: bool,
    has_bootstrap: bool,
    has_nui: bool,
    has_sass: bool,
    has_api: bool,
}

impl Ctx {
    fn ext(&self) -> &'static str {
        if self.has_ts { "ts" } else { "js" }
    }
    fn scss_ext(&self) -> &'static str {
        if self.has_sass { "scss" } else { "css" }
    }
    fn folders(&self) -> Vec<String> {
        let mut f = Vec::new();
        if self.has_vite {
            f.push("src/styles".into());
            f.push("src/components".into());
            f.push("src/utils".into());
            if self.has_nui { f.push("src/services".into()); }
            f.push("public".into());
        }
        f
    }
}

fn deps_to_json(deps: &[(String, String)]) -> String {
    if deps.is_empty() { "{}".into() }
    else {
        let items: Vec<String> = deps.iter().map(|(k, v)| format!("\"{k}\": {v}")).collect();
        format!("{{{}}}", items.join(", "))
    }
}

fn extract_features(features: &[String]) -> Ctx {
    Ctx {
        name: String::new(),
        version: "1.0.0".into(),
        has_vite: features.iter().any(|f| f == "vite"),
        has_ts: features.iter().any(|f| f == "typescript"),
        has_tailwind: features.iter().any(|f| f == "tailwind"),
        has_bootstrap: features.iter().any(|f| f == "bootstrap"),
        has_nui: features.iter().any(|f| f == "nui"),
        has_sass: features.iter().any(|f| f == "sass"),
        has_api: features.iter().any(|f| f == "api"),
    }
}

impl VanillaCodegen {
    fn generate_files(name: &str, version: &str, features: &[String]) -> Vec<(String, String)> {
        let mut ctx = extract_features(features);
        ctx.name = name.to_string();
        ctx.version = version.to_string();

        let mut files: Vec<(String, String)> = Vec::new();
        let e = ctx.ext();
        let se = ctx.scss_ext();

        if ctx.has_vite {
            // -- VITE MODE --
            let build = if ctx.has_ts { "tsc && vite build" } else { "vite build" };
            let mut devs = vec![("vite".into(), "\"^6.0.0\"".into())];
            let mut deps: Vec<(String, String)> = vec![];

            if ctx.has_ts {
                devs.push(("typescript".into(), "\"^5.7.0\"".into()));
                devs.push(("@types/node".into(), "\"^22.0.0\"".into()));
            }
            if ctx.has_tailwind {
                devs.push(("tailwindcss".into(), "\"^4.0.0\"".into()));
                devs.push(("postcss".into(), "\"^8.4.0\"".into()));
                devs.push(("autoprefixer".into(), "\"^10.4.0\"".into()));
            }
            if ctx.has_sass {
                devs.push(("sass".into(), "\"^1.80.0\"".into()));
            }
            if ctx.has_bootstrap {
                deps.push(("bootstrap".into(), "\"^5.3.3\"".into()));
            }

            files.push(("package.json".into(), format!(
                r#"{{ "name": "{name}", "private": true, "version": "{version}", "type": "module", "scripts": {{ "dev": "vite", "build": "{build}", "preview": "vite preview" }}, "dependencies": {deps_json}, "devDependencies": {devs_json} }}"#,
                deps_json = deps_to_json(&deps),
                devs_json = deps_to_json(&devs),
            )));

            if ctx.has_ts {
                files.push(("vite.config.ts".into(), Self::vite_config_ts()));
                files.push(("tsconfig.json".into(), Self::tsconfig()));
                files.push(("tsconfig.node.json".into(), Self::tsconfig_node()));
            } else {
                files.push(("vite.config.js".into(), Self::vite_config_js()));
            }

            if ctx.has_tailwind {
                files.push(("tailwind.config.js".into(), Self::tailwind_config()));
                files.push(("postcss.config.js".into(), Self::postcss_config()));
            }

            files.push(("index.html".into(), Self::vite_html(name, &ctx)));
            files.push((format!("src/main.{e}"), Self::vite_main(name, &ctx)));
            files.push((format!("src/styles/main.{se}"), Self::vite_styles(&ctx)));
            files.push((format!("src/components/app.{e}"), Self::vite_app(name, &ctx)));

            if ctx.has_nui {
                files.push((format!("src/components/button.{e}"), Self::nui_button(name)));
                files.push((format!("src/components/card.{e}"), Self::nui_card(name)));
            }

            if ctx.has_api {
                files.push((format!("src/services/api.{e}"), Self::api_client(&ctx)));
            }

            files.push((format!("src/utils/helpers.{e}"), Self::helpers()));
            files.push(("public/.gitkeep".into(), String::new()));

        } else {
            // -- PURE MODE (no build tools) --
            let bs_link = if ctx.has_bootstrap {
                "        <link href=\"https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css\" rel=\"stylesheet\" />\n"
            } else { "" };

            files.push(("index.html".into(), format!(
                "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n\
                 <meta charset=\"UTF-8\" />\n\
                 <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n\
                 <title>{name}</title>\n\
                 {bs_link}\
                 <link rel=\"stylesheet\" href=\"style.css\" />\n\
                 </head>\n<body>\n\
                 <div id=\"app\"></div>\n\
                 <script src=\"script.js\"></script>\n\
                 </body>\n</html>"
            )));

            if ctx.has_bootstrap {
                files.push(("style.css".into(),
                    "/* Bootstrap is loaded via CDN in index.html */\n/* Add your custom styles below */\n".to_string()));
                files.push(("script.js".into(),
                    "// Bootstrap is available globally via CDN\nconsole.log(\"Hello from \", document.title);\n".to_string()));
            } else {
                files.push(("style.css".into(), Self::pure_css()));
                files.push(("script.js".into(), Self::pure_js(name)));
            }
        }

        // Common files (always included)
        files.push((".gitignore".into(), Self::gitignore()));
        files.push((".env.example".into(), Self::env_example(name)));
        files.push((".editorconfig".into(), Self::editorconfig()));
        files.push(("README.md".into(), Self::readme(name, &ctx)));

        files
    }

    // --- PURE MODE RENDERERS ---
    fn pure_css() -> String {
        "*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }\n\
         :root { --color-primary: #646cff; --color-bg: #ffffff; --color-text: #213547; }\n\
         body { font-family: Inter, system-ui, sans-serif; color: var(--color-text);\n\
         background-color: var(--color-bg); line-height: 1.5; }\n\
         #app { max-width: 1280px; margin: 0 auto; padding: 2rem; text-align: center; }\n\
         .app-header h1 { color: var(--color-primary); }".to_string()
    }

    fn pure_js(name: &str) -> String {
        let pascal = name.to_upper_camel_case();
        format!(
            "const app = document.querySelector(\"#app\");\n\
             if (app) {{\n  app.innerHTML = `\n    <header class=\"app-header\">\n      <h1>{pascal}</h1>\n      <p>Built with MGPM</p>\n    </header>`;\n}}"
        )
    }

    // --- VITE MODE RENDERERS ---
    fn vite_config_js() -> String {
        "import { defineConfig } from \"vite\";\n\
         export default defineConfig({{\n  server: {{ port: 3000, open: true }},\n  preview: {{ port: 4173 }},\n}});".to_string()
    }

    fn vite_config_ts() -> String {
        "import { defineConfig } from \"vite\";\n\
         import { fileURLToPath, URL } from \"node:url\";\n\
         const __dirname = fileURLToPath(new URL(\".\", import.meta.url));\n\
         export default defineConfig({{\n  resolve: {{ alias: {{ \"@\": __dirname + \"/src\" }} }},\n  server: {{ port: 3000, open: true }},\n  preview: {{ port: 4173 }},\n}});".to_string()
    }

    fn tsconfig() -> String {
        include_str!("vanilla/tsconfig.json.hbs").to_string()
    }

    fn tsconfig_node() -> String {
        include_str!("vanilla/tsconfig.node.json.hbs").to_string()
    }

    fn tailwind_config() -> String {
        "/** @type {import('tailwindcss').Config} */\n\
         export default {{\n  content: [\"./src/**/*.{{html,js,ts,jsx,tsx}}\"],\n  theme: {{ extend: {{ }} }},\n  plugins: [],\n}};".to_string()
    }

    fn postcss_config() -> String {
        "export default {{\n  plugins: {{\n    tailwindcss: {{}},\n    autoprefixer: {{}},\n  }},\n}};".to_string()
    }

    fn vite_html(name: &str, ctx: &Ctx) -> String {
        let bs = if ctx.has_bootstrap {
            "        <link href=\"https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css\" rel=\"stylesheet\" />\n"
        } else { "" };
        let e = ctx.ext();
        format!(
            "<html lang=\"en\">\n<head>\n\
             <meta charset=\"UTF-8\" />\n\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n\
             <title>{name}</title>\n{bs}\
             </head>\n<body>\n<div id=\"app\"></div>\n\
             <script type=\"module\" src=\"/src/main.{e}\"></script>\n\
             </body>\n</html>"
        )
    }

    fn vite_main(name: &str, ctx: &Ctx) -> String {
        let bs = if ctx.has_bootstrap {
            "import \"bootstrap/dist/css/bootstrap.min.css\";\n"
        } else { "" };
        let se = ctx.scss_ext();
        let ts_label = if ctx.has_ts { " + TypeScript" } else { "" };
        let ts_assert = if ctx.has_ts { "<HTMLDivElement>" } else { "" };

        format!(
            "import \"./styles/main.{se}\";\n{bs}\
             import \"./components/app.{e}\";\n\
             \nconst app = document.querySelector{ts_assert}(\"#app\")!;\n\
             app.innerHTML = `<h1>Welcome to {name}</h1><p>MGPM + Vite{ts_label}</p>`;",
            e = ctx.ext()
        )
    }

    fn vite_styles(ctx: &Ctx) -> String {
        if ctx.has_tailwind {
            "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n".to_string()
        } else {
            Self::pure_css()
        }
    }

    fn vite_app(name: &str, _ctx: &Ctx) -> String {
        let pascal = name.to_upper_camel_case();
        format!(
            "const template = document.createElement(\"template\");\n\
             template.innerHTML = `\n<style>\n  .app-header {{\n    text-align: center;\n    padding: 2rem;\n  }}\n  .app-header h1 {{\n    color: var(--color-primary);\n  }}\n</style>\n\
             <header class=\"app-header\">\n  <h1>{pascal}</h1>\n  <slot></slot>\n</header>\n`;\n\
             export class AppElement extends HTMLElement {{\n\
             constructor() {{ super(); this.attachShadow({{ mode: \"open\" }}); }}\n\
             connectedCallback() {{\n    if (this.shadowRoot) {{\n      this.shadowRoot.appendChild(template.content.cloneNode(true));\n    }}\n  }}\n}}\n\
             customElements.define(\"app-root\", AppElement);"
        )
    }

    fn nui_button(name: &str) -> String {
        let pascal = name.to_upper_camel_case();
        format!(
            "const template = document.createElement(\"template\");\n\
             template.innerHTML = `\n<style>\n  .btn {{\n    display: inline-flex; align-items: center; justify-content: center;\n    padding: 0.5rem 1rem; border-radius: 0.375rem;\n    font-weight: 500; cursor: pointer; border: 1px solid transparent;\n    background-color: #646cff; color: white;\n  }}\n  .btn:hover {{ background-color: #535bf2; }}\n</style>\n  <button class=\"btn\"><slot></slot></button>\n`;\n\
             export class {pascal}Button extends HTMLElement {{\n\
             constructor() {{ super(); this.attachShadow({{ mode: \"open\" }}); }}\n\
             connectedCallback() {{\n    if (this.shadowRoot) {{\n      this.shadowRoot.appendChild(template.content.cloneNode(true));\n    }}\n  }}\n}}\n\
             customElements.define(\"{pascal}-button\", {pascal}Button);"
        )
    }

    fn nui_card(_name: &str) -> String {
        "const template = document.createElement(\"template\");\n\
         template.innerHTML = `\n<style>\n  .card {{\n    border: 1px solid #e2e8f0; border-radius: 0.5rem;\n    padding: 1.5rem; background: white;\n    box-shadow: 0 1px 3px rgba(0,0,0,0.1);\n  }}\n  .card-title {{\n    font-size: 1.25rem; font-weight: 600; margin-bottom: 0.5rem;\n  }}\n  .card-body {{ color: #64748b; }}\n</style>\n\
         <div class=\"card\">\n  <div class=\"card-title\"><slot name=\"title\"></slot></div>\n  <div class=\"card-body\"><slot></slot></div>\n</div>\n`;\n\
         export class AppCard extends HTMLElement {{\n\
         constructor() {{ super(); this.attachShadow({{ mode: \"open\" }}); }}\n\
         connectedCallback() {{\n    if (this.shadowRoot) {{\n      this.shadowRoot.appendChild(template.content.cloneNode(true));\n    }}\n  }}\n}}\n\
         customElements.define(\"app-card\", AppCard);".to_string()
    }

    fn api_client(ctx: &Ctx) -> String {
        let typed = if ctx.has_ts {
            "export async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {"
        } else {
            "export async function fetchJson(url, init = {}) {"
        };
        format!(
            "{typed}\n\
             const res = await fetch(url, init);\n\
             if (!res.ok) throw new Error(`HTTP ${{res.status}}: ${{res.statusText}}`);\n\
             return res.json();\n}}\n\
             export async function postJson(url, data) {{\n\
             return fetchJson(url, {{\n    method: \"POST\",\n    headers: {{ \"Content-Type\": \"application/json\" }},\n    body: JSON.stringify(data),\n  }});\n}}"
        )
    }

    fn helpers() -> String {
        "export function formatDate(date) {{\n  return new Intl.DateTimeFormat(\"en-US\", {{\n    year: \"numeric\", month: \"short\", day: \"numeric\"\n  }}).format(date);\n}}\n\
         export function cn(...classes) {{\n  return classes.filter(Boolean).join(\" \");\n}}".to_string()
    }

    // --- COMMON FILES ---
    fn gitignore() -> String {
        "node_modules\ndist\n.mgpm\n.env\n.env.local\n.env.*.local\n*.log\n.DS_Store\ncoverage\n".to_string()
    }

    fn env_example(name: &str) -> String {
        format!("# {name} — Environment Variables\n# Copy to .env and fill in values\n\n# VITE_API_URL=http://localhost:4000\n# VITE_APP_TITLE={name}")
    }

    fn editorconfig() -> String {
        "root = true\n\n[*]\nindent_style = space\nindent_size = 2\nend_of_line = lf\ncharset = utf-8\ntrim_trailing_whitespace = true\ninsert_final_newline = true\n\n[*.md]\ntrim_trailing_whitespace = false\n".to_string()
    }

    fn readme(name: &str, ctx: &Ctx) -> String {
        let pascal = name.to_upper_camel_case();
        let mode = if ctx.has_vite { "Vite" } else { "Vanilla" };
        let ts = if ctx.has_ts { " + TypeScript" } else { "" };
        let deps = if ctx.has_vite {
            "## Quick Start\n\n```bash\nnpm install\nnpm run dev\n```"
        } else {
            "## Quick Start\n\nOpen `index.html` in your browser."
        };
        format!("# {pascal}\n\n> Built with MGPM + {mode}{ts}\n\n{deps}\n\nMIT")
    }

    // --- ENGINE ---
    fn resolve_dest(ctx: &ScaffoldContext) -> Result<PathBuf, ScaffoldError> {
        let base = std::env::current_dir().map_err(|e| ScaffoldError::IoError {
            context: "current_dir".to_string(), source: e,
        })?;
        Ok(if ctx.project_path.is_absolute() { ctx.project_path.clone() }
           else { base.join(&ctx.project_path) })
    }

    fn write_files(dest: &Path, files: Vec<(String, String)>, force: bool) -> Result<Vec<PathBuf>, ScaffoldError> {
        let mut created = Vec::new();
        for (rel_path, content) in files {
            let dest_path = dest.join(&rel_path);
            if dest_path.exists() {
                if !force { return Err(ScaffoldError::PathExists(dest_path)); }
                if dest_path.is_file() { std::fs::remove_file(&dest_path)?; }
            }
            if let Some(parent) = dest_path.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::write(&dest_path, content)?;
            created.push(dest_path);
        }
        Ok(created)
    }
}

impl ScaffoldEngine for VanillaCodegen {
    fn name(&self) -> &str { "vanilla" }

    fn create_project(&self, ctx: &ScaffoldContext, force: bool) -> Result<ProjectCreated, ScaffoldError> {
        NameValidator::validate(&ctx.project_name).map_err(|e| {
            ScaffoldError::InvalidName(ctx.project_name.clone(), e.to_string())
        })?;
        let name = &ctx.project_name;
        let version = ctx.get_var("version").unwrap_or("1.0.0");
        let dest = Self::resolve_dest(ctx)?;
        let files = Self::generate_files(name, version, &ctx.features);
        let created = Self::write_files(&dest, files, force)?;
        Ok(ProjectCreated {
            name: name.clone(),
            path: dest,
            files_created: created,
            features: ctx.features.clone(),
        })
    }
}

pub fn template() -> Template {
    Template {
        name: "vanilla",
        description: "Vanilla JS/TS web app with optional Vite, Tailwind, Bootstrap, etc.",
        commands: &["web"],
        create_engine: || Box::new(VanillaCodegen),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn ctx(name: &str, features: Vec<&str>) -> ScaffoldContext {
        ScaffoldContext::new(name, PathBuf::from("/tmp/test"))
            .with_features(features.iter().map(|s| s.to_string()).collect())
    }

    // --- Default: pure HTML+CSS+JS ---
    #[test]
    fn test_default_pure_html_has_no_vite_no_ts() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &[]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"index.html"));
        assert!(paths.contains(&"style.css"));
        assert!(paths.contains(&"script.js"));
        assert!(!paths.contains(&"package.json"));
        assert!(!paths.contains(&"src/main.js"));
        assert_eq!(f.len(), 7);
    }

    #[test]
    fn test_pure_html_content() {
        let f = VanillaCodegen::generate_files("my-app", "1.0.0", &[]);
        let html = f.iter().find(|(p, _)| p.as_str() == "index.html").unwrap().1.clone();
        assert!(html.contains("<title>my-app</title>"));
        assert!(html.contains("style.css"));
        assert!(html.contains("script.js"));
        assert!(!html.contains("type=\"module\""));
    }

    #[test]
    fn test_pure_js_content() {
        let f = VanillaCodegen::generate_files("my-app", "1.0.0", &[]);
        let js = f.iter().find(|(p, _)| p.as_str() == "script.js").unwrap().1.clone();
        assert!(js.contains("MyApp"));
    }

    // --- --vite: Vite + JS ---
    #[test]
    fn test_vite_js() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".to_string()]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"package.json"));
        assert!(paths.contains(&"vite.config.js"));
        assert!(paths.contains(&"src/main.js"));
        assert!(!paths.contains(&"tsconfig.json"));
        assert_eq!(f.len(), 12);
    }

    #[test]
    fn test_vite_html_module_script() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".to_string()]);
        let html = f.iter().find(|(p, _)| p.as_str() == "index.html").unwrap().1.clone();
        assert!(html.contains("type=\"module\""));
        assert!(html.contains("src/main.js"));
    }

    // --- --vite --ts: Vite + TypeScript ---
    #[test]
    fn test_vite_ts() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".to_string(), "typescript".to_string()]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"vite.config.ts"));
        assert!(paths.contains(&"tsconfig.json"));
        assert!(paths.contains(&"src/main.ts"));
        assert!(!paths.contains(&"vite.config.js"));
        assert_eq!(f.len(), 14);
    }

    #[test]
    fn test_vite_html_ts_script() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".into(), "typescript".into()]);
        let html = f.iter().find(|(p, _)| p.as_str() == "index.html").unwrap().1.clone();
        assert!(html.contains("src/main.ts"));
    }

    // --- --tailwind ---
    #[test]
    fn test_tailwind_with_vite() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".into(), "tailwind".into()]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"tailwind.config.js"));
        assert!(paths.contains(&"postcss.config.js"));
        // CSS should be tailwind directives
        let css = f.iter().find(|(p, _)| p.as_str() == "src/styles/main.css").unwrap().1.clone();
        assert!(css.contains("@tailwind base"));
        assert_eq!(f.len(), 14);
    }

    // --- --bootstrap ---
    #[test]
    fn test_bootstrap_with_vite() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".into(), "bootstrap".into()]);
        let pkg = f.iter().find(|(p, _)| p.as_str() == "package.json").unwrap().1.clone();
        assert!(pkg.contains("bootstrap"));
        let html = f.iter().find(|(p, _)| p.as_str() == "index.html").unwrap().1.clone();
        assert!(html.contains("bootstrap.min.css"));
    }

    #[test]
    fn test_bootstrap_pure() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["bootstrap".into()]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"index.html"));
        assert_eq!(f.len(), 7);
        let html = f.iter().find(|(p, _)| p.as_str() == "index.html").unwrap().1.clone();
        assert!(html.contains("bootstrap.min.css"));
    }

    // --- --nui ---
    #[test]
    fn test_nui_with_vite() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".into(), "nui".into()]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/components/button.js"));
        assert!(paths.contains(&"src/components/card.js"));
    }

    // --- --sass ---
    #[test]
    fn test_sass_with_vite() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".into(), "sass".into()]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/styles/main.scss"));
        assert!(!paths.contains(&"src/styles/main.css"));
        let pkg = f.iter().find(|(p, _)| p.as_str() == "package.json").unwrap().1.clone();
        assert!(pkg.contains("sass"));
    }

    // --- --api ---
    #[test]
    fn test_api_with_vite() {
        let f = VanillaCodegen::generate_files("app", "1.0.0", &["vite".into(), "api".into()]);
        let paths: Vec<_> = f.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/services/api.js"));
    }

    // --- File counts ---
    #[test]
    fn test_file_counts() {
        assert_eq!(VanillaCodegen::generate_files("x", "1.0.0", &[]).len(), 7);           // pure
        assert_eq!(VanillaCodegen::generate_files("x", "1.0.0", &["vite".into()]).len(), 12); // vite
        assert_eq!(VanillaCodegen::generate_files("x", "1.0.0", &["vite".into(), "typescript".into()]).len(), 14); // vite+ts
        assert_eq!(VanillaCodegen::generate_files("x", "1.0.0", &["vite".into(), "tailwind".into()]).len(), 14);   // vite+tw
        assert_eq!(VanillaCodegen::generate_files("x", "1.0.0", &["vite".into(), "nui".into()]).len(), 14);        // vite+nui
        assert_eq!(VanillaCodegen::generate_files("x", "1.0.0", &["vite".into(), "api".into()]).len(), 13);        // vite+api
        assert_eq!(VanillaCodegen::generate_files("x", "1.0.0", &["vite".into(), "sass".into()]).len(), 12);       // vite+sass
    }

    // --- Engine integration ---
    #[test]
    fn test_engine_creates_pure_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        VanillaCodegen.create_project(&ctx, false).unwrap();
        assert!(dest.join("index.html").exists());
        assert!(dest.join("style.css").exists());
        assert!(dest.join("script.js").exists());
        assert!(!dest.join("package.json").exists());
    }

    #[test]
    fn test_engine_creates_vite_ts_project() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone())
            .with_features(vec!["vite".into(), "typescript".into()]);
        VanillaCodegen.create_project(&ctx, false).unwrap();
        assert!(dest.join("tsconfig.json").exists());
        assert!(dest.join("src/main.ts").exists());
        assert!(dest.join("vite.config.ts").exists());
        assert!(dest.join("package.json").exists());
    }

    #[test]
    fn test_engine_creates_all_flags() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        let ctx = ScaffoldContext::new("my-app", dest.clone())
            .with_features(vec!["vite".into(), "typescript".into(), "tailwind".into(), "bootstrap".into(), "nui".into(), "api".into()]);
        VanillaCodegen.create_project(&ctx, false).unwrap();
        assert!(dest.join("tailwind.config.js").exists());
        assert!(dest.join("src/components/button.ts").exists());
        assert!(dest.join("src/services/api.ts").exists());
        let html = std::fs::read_to_string(dest.join("index.html")).unwrap();
        assert!(html.contains("bootstrap.min.css"));
    }

    #[test]
    fn test_engine_fails_on_existing() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("index.html"), "x").unwrap();
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        let result = VanillaCodegen.create_project(&ctx, false);
        assert!(matches!(result, Err(ScaffoldError::PathExists(_))));
    }

    #[test]
    fn test_engine_force_overwrites() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("index.html"), "old").unwrap();
        let ctx = ScaffoldContext::new("my-app", dest.clone());
        VanillaCodegen.create_project(&ctx, true).unwrap();
        let c = std::fs::read_to_string(dest.join("index.html")).unwrap();
        assert!(c.contains("my-app"));
        assert!(!c.contains("old"));
    }

    #[test]
    fn test_invalid_name() {
        let ctx = ScaffoldContext::new("", PathBuf::from("/tmp/x"));
        let result = VanillaCodegen.create_project(&ctx, false);
        assert!(matches!(result, Err(ScaffoldError::InvalidName(_, _))));
    }

    #[test]
    fn test_nui_button_contains_name() {
        let r = VanillaCodegen::nui_button("my-app");
        assert!(r.contains("MyAppButton"));
        assert!(r.contains("MyApp-button"));
    }

    #[test]
    fn test_api_client_ts() {
        let ctx = Ctx { name: "x".into(), version: "1.0.0".into(), has_vite: true, has_ts: true, has_tailwind: false, has_bootstrap: false, has_nui: false, has_sass: false, has_api: true };
        let r = VanillaCodegen::api_client(&ctx);
        assert!(r.contains("<T>"));
    }
}
