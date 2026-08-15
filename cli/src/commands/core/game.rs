use std::sync::Arc;
use anyhow::Result;
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;
use std::path::PathBuf;

const OPTIMIZER_PKG: &str = "optimizer";

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| {
        anyhow::anyhow!("failed to resolve current working directory — has it been deleted?: {e}")
    })?;
    let root = super::shared::find_project_root(&cwd)?.ok_or_else(|| {
        anyhow::anyhow!(
            "No MegaGate game project found (missing mg.toml with ecosystem = \"game\" or project.godot/Packages/manifest.json/.uproject/Cargo.toml in the current project)"
        )
    })?;
    Ok(root)
}

fn game_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Game, None, None)
        .expect("game adapter always available in game core build")
}

async fn ensure_optimizer_template(root: &PathBuf) -> Result<()> {
    super::hardware::materialize_template(root, super::hardware::OPTIMIZER_PKG).await
}

/// bevy (rust): thêm dep path `mg-optimizer = { path = "./optimizer" }` vào root Cargo.toml
/// để game code gọi FFI thật. godot/unity/unreal không có cargo → bỏ qua.
fn hook_optimizer_dep(root: &PathBuf) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if !manifest.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&manifest)?;
    let mut v: toml::Value = toml::from_str(&content)?;
    let deps = v["dependencies"]
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("root Cargo.toml missing [dependencies]"))?;
    if deps.contains_key("mg-optimizer") {
        return Ok(());
    }
    deps.insert(
        "mg-optimizer".to_string(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "path".to_string(),
            toml::Value::String("./optimizer".to_string()),
        )])),
    );
    std::fs::write(&manifest, toml::to_string_pretty(&v)?)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
) -> Result<()> {
    let root = project_root()?;
    let adapter = game_adapter();

    // optimizer/bench: không phải crate registry — materialize template + hook dep,
    // không gửi qua game adapter (bevy `cargo add optimizer` sẽ fail vì crate không tồn tại).
    let mut adapter_pkgs = Vec::new();
    for pkg in &packages {
        if pkg == OPTIMIZER_PKG {
            ensure_optimizer_template(&root).await?;
            hook_optimizer_dep(&root)?;
        } else {
            adapter_pkgs.push(pkg.clone());
        }
    }
    if adapter_pkgs.is_empty() {
        return Ok(());
    }

    super::shared::add(
        &*adapter,
        &root,
        adapter_pkgs,
        version,
        dev,
        exact,
        optional,
        peer,
        no_save,
        true,
        global,
    )
    .await
}

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = game_adapter();
    super::shared::remove(&*adapter, &root, packages, true).await
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let adapter = game_adapter();
    super::shared::list(&*adapter, &root).await
}

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = game_adapter();
    super::shared::update(&*adapter, &root, packages, install).await
}

pub async fn install(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = game_adapter();

    // optimizer: materialize + hook dep; không gửi qua adapter (không phải registry crate)
    let mut adapter_pkgs = Vec::new();
    for pkg in &packages {
        if pkg == OPTIMIZER_PKG {
            ensure_optimizer_template(&root).await?;
            hook_optimizer_dep(&root)?;
        } else {
            adapter_pkgs.push(pkg.clone());
        }
    }

    for pkg in &adapter_pkgs {
        let spinner = mg_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mg_types::PackageName::new(pkg)?;
        let opts = mg_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    super::shared::install_with_adapter(
        &*adapter,
        &root,
        "mg add",
        false,
        mg_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::game::GameWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        if let Some(fw) = config.frameworks.first() {
            // Registry-first: fetch layer game/<fw> nếu chưa có (template.toml+sources);
            // fetch fail → fallback generator procedural sẵn có.
            crate::commands::template::ensure_layer(&format!("game/{fw}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success("Game project created. Run `mg add-game <pkg>` or `mg install-game` next.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template_layer_ready(rel: &str) -> bool {
        let root = crate::scaffold::template_root::TemplateRoot::resolve(rel);
        root.exists("template.toml") && root.exists("sources")
    }

    #[test]
    fn hook_optimizer_dep_adds_path_dep_to_bevy_manifest() {
        let dir = std::env::temp_dir().join(format!(
            "mg-game-hook-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbevy = \"0.14\"\n",
        )
        .unwrap();

        hook_optimizer_dep(&dir).unwrap();
        let after = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(after.contains("mg-optimizer"), "dep must be added: {after}");
        assert!(after.contains("./optimizer"), "path dep: {after}");
        assert!(
            toml::from_str::<toml::Value>(&after).is_ok(),
            "manifest stays valid"
        );

        // idempotent: chạy lại không thêm trùng
        hook_optimizer_dep(&dir).unwrap();
        let twice = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert_eq!(
            twice.matches("mg-optimizer").count(),
            1,
            "no duplicate dep: {twice}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scaffold_materializes_framework_structure() {
        if !template_layer_ready("game/bevy") {
            eprintln!("skipped: game/bevy template layer not available offline (registry-first)");
            return;
        }
        for fw in ["bevy", "godot", "unity", "unreal"] {
            let dir = std::env::temp_dir().join(format!(
                "mg-game-scaffold-{fw}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let prev = std::env::current_dir().unwrap();
            std::env::set_current_dir(&dir).unwrap();
            let config = crate::wizard::engine::ScaffoldConfig {
                core: "game".to_string(),
                sub_type: String::new(),
                frameworks: vec![fw.to_string()],
                project_name: "game-demo".to_string(),
                features: vec![],
                template_dir: PathBuf::new(),
            };
            let res = crate::scaffold::processor::Scaffolder::scaffold(&config);
            std::env::set_current_dir(&prev).unwrap();
            res.expect("scaffold game");
            let base = dir.join("game-demo");
            assert!(base.join("mg.toml").exists(), "{fw}: mg.toml missing");

            let expected: &[&str] = match fw {
                "bevy" => &[
                    "Cargo.toml",
                    "src/main.rs",
                    "src/components.rs",
                    "src/game_state.rs",
                    "src/systems/mod.rs",
                    "src/systems/setup.rs",
                    "src/systems/movement.rs",
                    "assets/README.md",
                ],
                "godot" => &[
                    "project.godot",
                    "scenes/Main.tscn",
                    "scripts/Main.gd",
                    "scripts/Player.gd",
                    "assets/README.md",
                ],
                "unity" => &["Packages/manifest.json", "Assets/Scripts/Bootstrap.cs"],
                "unreal" => &[
                    "game-demo.uproject",
                    "Config/DefaultEngine.ini",
                    "Source/game-demo/game-demo.Build.cs",
                    "Source/game-demo/game-demo.cpp",
                    "Source/game-demo.Target.cs",
                    "Source/game-demoEditor.Target.cs",
                    "Content/README.md",
                ],
                _ => unreachable!(),
            };
            for rel in expected {
                assert!(
                    base.join(rel).exists(),
                    "{fw}: expected file '{rel}' not materialized"
                );
            }
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn hook_optimizer_dep_skips_non_cargo_projects() {
        let dir = std::env::temp_dir().join(format!(
            "mg-game-hook-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // godot project: không có root Cargo.toml
        std::fs::write(dir.join("project.godot"), "# demo\n").unwrap();
        hook_optimizer_dep(&dir).unwrap();
        assert!(!dir.join("Cargo.toml").exists(), "no Cargo.toml created");
        std::fs::remove_dir_all(&dir).ok();
    }
}
