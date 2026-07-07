use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct DockerInstaller;

impl Installer for DockerInstaller {
    fn name(&self) -> &str {
        "docker"
    }

    fn description(&self) -> &str {
        "Docker multi-stage build"
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let dockerfile = r#"FROM node:22-alpine AS build
WORKDIR /app
COPY package.json ./
RUN mg install
COPY . .
RUN mg build

FROM node:22-alpine AS production
WORKDIR /app
COPY --from=build /app/package.json ./
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist
EXPOSE 3000
CMD ["node", "dist/index.js"]
"#;

        let dockerignore = r#"node_modules
dist
.mg
.git
*.md
"#;

        let compose = r#"services:
  app:
    build:
      context: .
      target: production
    ports:
      - "3000:3000"
    environment:
      - NODE_ENV=production
    volumes:
      - .:/app
      - /app/node_modules
"#;

        let files = vec![
            write_file(project_dir, "Dockerfile", dockerfile)?,
            write_file(project_dir, ".dockerignore", dockerignore)?,
            write_file(project_dir, "docker-compose.yml", compose)?,
        ];

        Ok(InstallResult {
            installer_name: "docker".to_string(),
            files_created: files,
            dependencies_added: vec![],
        })
    }
}
