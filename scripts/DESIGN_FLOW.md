# MegaGate CLI Design

## Core names

```
web    → brew install megagate-web
game   → brew install megagate-game
app    → brew install megagate-app
ai     → brew install megagate-ai
clo    → brew install megagate-clo
cicd   → brew install megagate-cicd
lib    → brew install megagate-lib
```

Full build: `brew install megagate` → includes all 7 cores.

---

## Quick create (non-interactive)

```
mg create-<core> <framework>[@<version>] <project-name>
```

Examples:

```
mg create-web next-app@latest my-app
mg create-web react-vite@latest my-app
mg create-web express@latest api-server
mg create-game bevy@latest my-game
mg create-game godot@4.3 my-game
mg create-ai langchain@latest my-agent
mg create-ai openai-functions@1.0 my-bot
mg create-clo pulumi@latest my-infra
mg create-clo terraform@1.9 my-stack
mg create-cicd github-actions@latest my-pipeline
mg create-cicd argocd@latest my-deploy
mg create-iot platformio@latest my-firmware
mg create-app flutter@3.22 my-mobile-app
mg create-lib my-library
```

- Nếu không có `@version` → mặc định `@latest`
- `next-app` = `next-app@latest` (same behavior)
- Framework version tracking: theo dõi GitHub releases (xử lý sau)
- Tất cả flag framework-specific (--ts, --tailwindcss, --zod, --prisma) sẽ được xử lý sau ở phần scaffold

---

## Interactive init

### Full build (`brew install megagate`)

```
mg init
│
├── Pick core (7 options):
│   ├── web   → Web application
│   ├── game  → Game
│   ├── ai    → AI agent / ML project
│   ├── clo   → Cloud infrastructure
│   ├── cicd  → CI/CD pipeline
│   ├── app   → Mobile / Desktop app
│   └── lib   → Library
│
├── Core-specific wizard
│   └── Web: type → framework → project name → features
│       Other cores: just project name (wizard TBD)
│
└── Scaffold + .megagate/project.toml
```

### Single-core build (`brew install megagate-<core>`)

```
mg init
│
├── Auto core (skip pick menu)
│
├── Core-specific wizard
│
└── Scaffold + .megagate/project.toml
```

---

## Global flags

```
--core <CORE>    Target core (web, game, ai, clo, cicd, iot, app, lib)
-h, --help       Print help
-V, --version    Print version
```

---

## Full help output

```
mg help

MegaGate - Universal Package Manager
Usage: mg [OPTIONS] [COMMAND]

Commands:
  init         Interactive project wizard
  install      Install all dependencies
  info         Show package information
  search       Search for packages
  outdated     Check for outdated packages
  audit        Audit packages for vulnerabilities
  add          Add a dependency
  remove       Remove a dependency
  update       Update packages
  list         List installed packages
  create-web   Scaffold a new web project
  create-game  Scaffold a new game project
  create-ai    Scaffold a new AI project
  create-clo   Scaffold a new cloud project
  create-cicd  Scaffold a new CI/CD project
  create-iot   Scaffold a new IoT project
  create-app   Scaffold a new app project
  create-lib   Scaffold a new library project
  add-web      Add web dependency
  add-game     Add game dependency
  add-ai       Add AI dependency
  add-clo      Add cloud dependency
  add-cicd     Add CI/CD dependency
  add-iot      Add IoT dependency
  add-app      Add app dependency
  add-lib      Add library dependency
  remove-web   Remove web dependency
  remove-game  Remove game dependency
  remove-ai    Remove AI dependency
  remove-clo   Remove cloud dependency
  remove-cicd  Remove CI/CD dependency
  remove-iot   Remove IoT dependency
  remove-app   Remove app dependency
  remove-lib   Remove library dependency
  list-web     List web packages
  list-game    List game packages
  list-ai      List AI packages
  list-clo     List cloud packages
  list-cicd    List CI/CD packages
  list-iot     List IoT packages
  list-app     List app packages
  list-lib     List library packages
  update-web   Update web packages
  update-game  Update game packages
  update-ai    Update AI packages
  update-clo   Update cloud packages
  update-cicd  Update CI/CD packages
  update-iot   Update IoT packages
  update-app   Update app packages
  update-lib   Update library packages
  help         Print this message or the help of the given subcommand(s)
```

---

## Per-command flags

### `init`

```
mg init [OPTIONS]
  -t, --template <TEMPLATE>    Skip wizard, use template
      --core <CORE>            Target core
```

### `install`

```
mg install [PACKAGES]... [OPTIONS]
      --core <CORE>            Target core
```

### `info`

```
mg info <PACKAGE> [OPTIONS]
      --core <CORE>            Target core
```

### `search`

```
mg search <QUERY> [OPTIONS]
      --core <CORE>            Target core
```

### `outdated`

```
mg outdated [OPTIONS]
      --core <CORE>            Target core
```

### `audit`

```
mg audit [OPTIONS]
      --core <CORE>            Target core
```

### `add` (single-core mode — có .megagate/)

```
mg add <PACKAGE> [OPTIONS]
  -v, --version <VERSION>      Version specifier (default: latest)
  -D, --dev                    Add to devDependencies
  -E, --exact                  Save exact version (no ^ or ~)
  -O, --optional               Add to optionalDependencies
  -P, --peer                   Add to peerDependencies
      --no-save                Do not save to manifest
  -g, --global                 Install globally
      --core <CORE>            Target core
```

### `add-<core>` (global mode — không có .megagate/)

```
mg add-web <PACKAGE> [OPTIONS]
  -D, --dev                    Add to devDependencies
  -E, --exact                  Save exact version (no ^ or ~)
  -O, --optional               Add to optionalDependencies
  -P, --peer                   Add to peerDependencies
      --no-save                Do not save to manifest
  -g, --global                 Install globally
      --core <CORE>            Target core
```

### `remove` / `remove-<core>`

```
mg remove <PACKAGE> [OPTIONS]
      --core <CORE>            Target core

mg remove-web <PACKAGE> [OPTIONS]
      --core <CORE>            Target core
```

### `list` / `list-<core>`

```
mg list [OPTIONS]
      --core <CORE>            Target core

mg list-web [OPTIONS]
      --core <CORE>            Target core
```

### `update` / `update-<core>`

```
mg update [PACKAGES]... [OPTIONS]
      --core <CORE>            Target core

mg update-web [PACKAGES]... [OPTIONS]
      --core <CORE>            Target core
```

### `create-<core>`

```
mg create-web <FRAMEWORK> <PROJECT_NAME> [OPTIONS]
      --core <CORE>            Target core
```

---

## Package management commands

### Global mode (không có .megagate/ context)

Khi user đứng ngoài project, không có `.megagate/`:

```
mg add-web react@^19.0.0          ← dependencies
mg add-web -D vitest@latest       ← devDependencies
mg add-web -E lodash@4.17.21      ← exact, ghi "4.17.21" không "^4.17.21"
mg add-web -O debug               ← optionalDependencies
mg add-web -P react-dom           ← peerDependencies
mg add-web -g typescript          ← global store
mg add-web --no-save prettier     ← chỉ resolve, không ghi manifest
mg add-game bevy@0.14
mg add-ai torch@latest
mg add-clo -D pulumi@latest
mg add-cicd argocd@stable
mg add-iot platformio@latest
mg add-app flutter@3.22
mg add-lib lodash

mg remove-web lodash
mg remove-game bevy
mg remove-clo pulumi
mg remove-cicd argocd
mg remove-lib is-odd

mg list-web
mg list-game
mg list-ai
mg list-clo
mg list-cicd
mg list-iot
mg list-app
mg list-lib

mg update-web
mg update-game
mg update-ai
mg update-clo
mg update-cicd
mg update-iot
mg update-app
mg update-lib

mg install
mg info lodash
mg search react
mg outdated
mg audit
```

### Single-core mode (có .megagate/ xác định core)

Khi đã init project → `.megagate/project.toml` có ecosystem:

```
cd my-web-project

mg add react@^19.0.0              ← tự động biết core = web
mg add -D vitest@latest           ← devDependencies
mg add -E lodash@4.17.21          ← exact version
mg add -O debug                   ← optionalDependencies
mg add -P react-dom               ← peerDependencies
mg add -g typescript              ← global
mg add --no-save prettier         ← không ghi
mg remove lodash
mg list
mg update
```

Hoặc dùng `--core` override:

```
mg --core game add bevy            ← ghi đè sang game core
mg --core lib add lodash
```

### Version specifier syntax

Version specifier có thể để inline (npm-style) hoặc qua flag:

```
mg add-web react@^19.0.0          ← inline (recommended)
mg add-web react --version ^19.0.0  ← flag override
mg add-web react                  ← latest (mặc định)
mg add-web react@latest           ← latest (tường minh)
```

Khi có cả inline và `--version` thì `--version` thắng.

---

## Website wizard tree (mẫu)

```
What type?
├── Frontend → framework (next, react-vite, vue-vite, nuxt, sveltekit, angular, astro, vanilla)
├── Backend  → language
│              ├── node → framework (express, fastify, nestjs, hono, trpc)
│              ├── php  → framework (laravel, symfony)
│              ├── java → framework (spring-boot, quarkus)
│              ├── go   → framework (gin, echo, fiber)
│              ├── python → framework (fastapi, django, flask)
│              └── rust → framework (axum, actix-web)
├── Fullstack → stack (nextjs, nuxt, sveltekit, remix, custom)
└── Monorepo  → FE framework + BE framework
```

Mỗi core khác tự định nghĩa tree riêng. Web là reference.

---

## File scaffold output

```
my-app/
├── .megagate/
│   └── project.toml              → ecosystem = "web"
├── package.json                  → name, dependencies, devDependencies
├── tsconfig.json                 (if --ts)
├── tailwind.config.ts            (if --tailwindcss)
├── ...
└── README.md
```
