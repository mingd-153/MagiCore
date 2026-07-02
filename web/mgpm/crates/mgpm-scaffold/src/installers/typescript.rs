use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct TypeScriptInstaller;

impl Installer for TypeScriptInstaller {
    fn name(&self) -> &str {
        "typescript"
    }

    fn description(&self) -> &str {
        "TypeScript configuration (tsconfig.json)"
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![("typescript", "^5.7.0"), ("@types/node", "^22.0.0")]
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let tsconfig = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022"],
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"]
    }
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
"#;

        let tsconfig_node = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "noEmit": true,
    "strict": true,
    "skipLibCheck": true
  },
  "include": ["vite.config.ts", "vitest.config.ts"]
}
"#;

        let files = vec![
            write_file(project_dir, "tsconfig.json", tsconfig)?,
            write_file(project_dir, "tsconfig.node.json", tsconfig_node)?,
        ];

        Ok(InstallResult {
            installer_name: "typescript".to_string(),
            files_created: files,
            dependencies_added: vec!["typescript".to_string(), "@types/node".to_string()],
        })
    }
}
