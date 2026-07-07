pub mod styles;

use crate::error::ScaffoldError;
use crate::installers::Installer;
use crate::validate::name::NameValidator;
use styles::{style_header, style_success};

#[derive(Debug, Default)]
pub struct PromptResult {
    pub use_typescript: bool,
    pub use_tailwind: bool,
    pub use_eslint: bool,
    pub use_prettier: bool,
    pub use_vitest: bool,
    pub use_docker: bool,
    pub use_ci: bool,
    pub use_husky: bool,
    pub use_git: bool,
    pub install_deps: bool,
    pub git_init: bool,
    pub package_manager: PackageManager,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    #[default]
    Mg,
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    pub fn as_command(&self) -> &'static str {
        match self {
            PackageManager::Mg => "mg",
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        }
    }

    pub fn install_command(&self) -> String {
        match self {
            PackageManager::Mg => "mg install".to_string(),
            PackageManager::Npm => "npm install".to_string(),
            PackageManager::Pnpm => "pnpm install".to_string(),
            PackageManager::Yarn => "yarn install".to_string(),
            PackageManager::Bun => "bun install".to_string(),
        }
    }
}

pub struct PromptHandler;

impl Default for PromptHandler {
    fn default() -> Self {
        Self
    }
}

impl PromptHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn ask_project_name(default: &str) -> Result<String, ScaffoldError> {
        use dialoguer::Input;
        let name: String = Input::new()
            .with_prompt(style_header("Project name"))
            .default(default.to_string())
            .validate_with(|input: &String| -> Result<(), String> {
                NameValidator::validate(input).map_err(|e| e.to_string())
            })
            .interact()
            .map_err(|e| ScaffoldError::Generic(e.to_string()))?;
        Ok(name)
    }

    pub fn ask_typescript() -> Result<bool, ScaffoldError> {
        use dialoguer::Confirm;
        let ts = Confirm::new()
            .with_prompt(style_success("TypeScript?"))
            .default(true)
            .interact()
            .map_err(|e| ScaffoldError::Generic(e.to_string()))?;
        Ok(ts)
    }

    pub fn ask_features(available: &[&dyn Installer]) -> Result<Vec<String>, ScaffoldError> {
        use dialoguer::MultiSelect;
        let items: Vec<String> = available
            .iter()
            .map(|i| format!("{} — {}", i.name(), i.description()))
            .collect();
        let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
        let selections = MultiSelect::new()
            .with_prompt(style_success("Which features?"))
            .items(&refs)
            .interact()
            .map_err(|e| ScaffoldError::Generic(e.to_string()))?;
        let selected: Vec<String> = selections
            .iter()
            .map(|&i| available[i].name().to_string())
            .collect();
        Ok(selected)
    }

    pub fn ask_git_init() -> Result<bool, ScaffoldError> {
        use dialoguer::Confirm;
        let git = Confirm::new()
            .with_prompt(style_success("Git init?"))
            .default(true)
            .interact()
            .map_err(|e| ScaffoldError::Generic(e.to_string()))?;
        Ok(git)
    }

    pub fn ask_install_deps() -> Result<bool, ScaffoldError> {
        use dialoguer::Confirm;
        let deps = Confirm::new()
            .with_prompt(style_success("Install dependencies?"))
            .default(true)
            .interact()
            .map_err(|e| ScaffoldError::Generic(e.to_string()))?;
        Ok(deps)
    }

    pub fn ask_package_manager() -> Result<PackageManager, ScaffoldError> {
        use dialoguer::Select;
        let items = &["mg (default)", "npm", "pnpm", "yarn", "bun"];
        let selection = Select::new()
            .with_prompt(style_success("Package manager?"))
            .items(items)
            .default(0)
            .interact()
            .map_err(|e| ScaffoldError::Generic(e.to_string()))?;
        match selection {
            0 => Ok(PackageManager::Mg),
            1 => Ok(PackageManager::Npm),
            2 => Ok(PackageManager::Pnpm),
            3 => Ok(PackageManager::Yarn),
            4 => Ok(PackageManager::Bun),
            _ => Ok(PackageManager::Mg),
        }
    }
}
