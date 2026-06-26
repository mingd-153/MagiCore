# MegaGate Package Manager - Detailed Specification

Based on pnpm (disk efficiency, security, monorepo) + Bun (speed, DX, all-in-one)
Original implementation - no copied code.

## Folder Structure

```
webapp-core/
├── src/
│   ├── types/              # Single source of truth
│   ├── config/             # Config system (TOML)
│   ├── store/              # Store abstraction + backends
│   ├── security/           # Security-first features
│   ├── fetcher/            # Network + streaming extract
│   ├── resolver/           # Version resolution + conflicts
│   ├── linker/             # Linking strategies
│   ├── lockfile/           # Lockfile v1/v2 operations
│   ├── installer/          # Orchestration
│   ├── workspace/          # Monorepo support
│   ├── runtime/            # Dev toolchain (Phase 2)
│   └── cli/                # Commands
├── tests/
├── benchmarks/
└── scripts/
```

## Phase 0: Types & Config (Week 1)

### 0.1 src/types/index.ts
```typescript
// Core types - no external deps
export interface MegagateConfig {
  storeDir: string;
  registry: string;
  minimumReleaseAgeHours: number;  // 24 default
  approveBuilds: boolean;          // true default
  lockdownMode: boolean;           // false default
  linkStrategy: 'hardlink' | 'symlink' | 'copy';
  maxConcurrency: number;          // 16
  offlineMode: boolean;
  preferOffline: boolean;
}

export interface PackageManifest {
  name: string;
  version: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  peerDependenciesMeta?: Record<string, { optional: boolean }>;
  bin?: Record<string, string>;
  scripts?: Record<string, string>;
  engines?: { node?: string; megagate?: string };
  files?: string[];
  main?: string;
  module?: string;
  types?: string;
  exports?: any;
  sideEffects?: boolean | string[];
  megagate?: {
    lockdown?: boolean;
    entryPoints?: string[];
    testEntryPoints?: string[];
  };
}

export interface LockfileV1 {
  version: 1;
  lockfileVersion: 1;
  packages: Record<string, LockedPackage>;
  importers: Record<string, ImporterDeps>;
  store: { dir: string; layoutVersion: 1 };
  metadata: {
    createdAt: string;
    megagateVersion: string;
    contentHash: string;  // SHA-256 of normalized deps
  };
}

export interface LockedPackage {
  name: string;
  version: string;
  integrity: string;       // sha512-...
  resolved: string;        // tarball URL
  size: number;
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  bin?: Record<string, string>;
  engines?: Record<string, string>;
  provenance?: ProvenanceInfo;
  approvedBuilds?: string[];
  publishTime?: string;
}

export interface ProvenanceInfo {
  repositoryUrl?: string;
  commitHash?: string;
  builderId?: string;
  signature?: string;
}

export interface ImporterDeps {
  dependencies: Record<string, string>;
  devDependencies: Record<string, string>;
  optionalDependencies: Record<string, string>;
}

export interface ResolvedDependency {
  name: string;
  version: string;
  integrity: string;
  resolved: string;
  size: number;
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  bin?: Record<string, string>;
  engines?: Record<string, string>;
  publishTime?: string;
}

export interface InstallOptions {
  frozenLockfile?: boolean;
  production?: boolean;
  registry?: string;
  storeDir?: string;
}

export interface FetchResult {
  tarballPath: string;
  extractPath: string;
  integrity: string;
  size: number;
}

export interface LinkOptions {
  cwd?: string;
  storeDir?: string;
  production?: boolean;
}

export interface WorkspaceConfig {
  packages: string[];
  catalog?: Record<string, string>;
  overrides?: Record<string, string>;
  linkWorkspacePackages?: 'shallow' | 'deep' | false;
}
```

### 0.2 src/config/index.ts
- TOML parser (no external deps - simple implementation)
- Load: `megagate.toml` (project) + `~/.megagaterc` (global)
- Schema validation
- Environment variable overrides

### 0.3 Tasks
- [ ] Create folder structure
- [ ] Write types/index.ts
- [ ] Write config/index.ts (TOML parser)
- [ ] Unit tests for config loading

## Phase 1: Store Abstraction (Week 1-2)

### 1.1 src/store/index.ts - Interface
```typescript
export interface PackageRef {
  name: string;
  version: string;
}

export interface IntegrityInfo {
  integrity: string;  // sha512-...
  size: number;
}

export interface PackageMetadata {
  integrity: string;
  size: number;
  extractedAt: string;
  publishTime?: string;
  approvedBuilds?: string[];
}

export interface StoreBackend {
  init(): Promise<void>;
  exists(pkg: PackageRef): Promise<boolean>;
  getPath(pkg: PackageRef): string;
  writeTarball(pkg: PackageRef, stream: Readable): Promise<IntegrityInfo>;
  readTarball(pkg: PackageRef): Promise<Readable>;
  writeManifest(pkg: PackageRef, manifest: PackageManifest): Promise<void>;
  readManifest(pkg: PackageRef): Promise<PackageManifest | null>;
  writeMetadata(pkg: PackageRef, meta: PackageMetadata): Promise<void>;
  readMetadata(pkg: PackageRef): Promise<PackageMetadata | null>;
  createHardlink(pkg: PackageRef, target: string): Promise<void>;
  createSymlink(pkg: PackageRef, target: string): Promise<void>;
  remove(pkg: PackageRef): Promise<void>;
  prune(referenced: Set<string>): Promise<PruneResult>;
  verifyIntegrity(pkg: PackageRef): Promise<boolean>;
}

export interface PruneResult {
  removed: number;
  freedBytes: number;
}
```

### 1.2 src/store/fsBackend.ts - FS Implementation
- Layout: `~/.megagate/store/v1/{files,nodes}/`
- `files/pkg-name-version.tgz` + `.sha512`
- `nodes/pkg/name/version/{package.json, node_modules/, .megagate-meta.json}`
- Hardlink/symlink/copy strategies
- Streaming tarball extract (tar-fs or native)

### 1.3 src/store/sqliteIndex.ts - O(1) Lookup (Phase 3)
- Table: packages(name, version, integrity, size, publish_time, metadata_json)
- Index on (name, version), integrity
- Background indexer

### 1.4 Tasks
- [ ] Write store/index.ts (interface)
- [ ] Write store/fsBackend.ts (full implementation)
- [ ] Write store/sqliteIndex.ts (stub for now)
- [ ] Unit tests: exists, write/read tarball, link strategies, prune

## Phase 1: Security Module (Week 2) - CRITICAL

### 2.1 src/security/index.ts
```typescript
export interface SecurityContext {
  config: MegagateConfig;
  store: StoreBackend;
}

export class SecurityManager {
  constructor(private ctx: SecurityContext) {}

  // minimumReleaseAge: block packages published < 24h ago
  async checkReleaseAge(pkg: LockedPackage): Promise<void>;

  // approve-builds: skip lifecycle scripts by default
  async checkBuildApproval(pkg: LockedPackage, scriptName: string): Promise<boolean>;

  // Lockdown mode: extra checks for vanilla projects
  async validateLockdown(pkg: LockedPackage): Promise<void>;

  // Dependency confusion protection
  async checkConfusion(pkg: LockedPackage): Promise<void>;

  // Run lifecycle scripts in sandbox
  async runLifecycleScript(
    pkg: LockedPackage,
    script: 'prepare' | 'preinstall' | 'postinstall' | 'prepublish',
    cwd: string
  ): Promise<void>;
}
```

### 2.2 src/security/approveBuilds.ts
- Default: DENY all lifecycle scripts
- Allowlist: `megagate security approve-builds <pkg> [--script postinstall]`
- Store approved list in lockfile `approvedBuilds: string[]`
- Run scripts with: no network, no fs write outside cwd, timeout 60s

### 2.3 src/security/minimumReleaseAge.ts
- Check `publishTime` from registry metadata
- Block if `Date.now() - publishTime < minHours * 3600000`
- Configurable via `megagate.toml` + CLI flag `--ignore-minimum-age`

### 2.4 src/security/lockdown.ts
- No native addons (scan for `.node` files, `binding.gyp`)
- No eval/Function constructor (static AST check)
- CSP-compatible: no inline script requirements
- Enforce `sideEffects: false` in package.json

### 2.5 Tasks
- [ ] Write security/index.ts
- [ ] Write security/approveBuilds.ts
- [ ] Write security/minimumReleaseAge.ts
- [ ] Write security/lockdown.ts
- [ ] Integrate into resolver/fetcher/installer
- [ ] CLI: `megagate security approve-builds`, `megagate security audit`
- [ ] Unit tests for each check

## Phase 1: Fetcher - Streaming + Pool (Week 2)

### 3.1 src/fetcher/pool.ts - Connection Pool
```typescript
import { Agent } from 'undici';

export function createFetchPool(maxConcurrency = 16): Agent {
  return new Agent({
    connections: maxConcurrency,
    pipelining: true,
    keepAliveTimeout: 30000,
    keepAliveMaxTimeout: 60000,
    connect: { timeout: 10000 },
  });
}

export interface FetchOptions {
  url: string;
  headers?: Record<string, string>;
  timeout?: number;
  retries?: number;
}
```

### 3.2 src/fetcher/streamExtract.ts - Zero Memory Buffer
```typescript
import { pipeline } from 'stream/promises';
import { createHash } from 'crypto';
import { createExtract } from 'tar-fs';  // or custom implementation
import { Readable } from 'stream';

export async function streamDownloadExtract(
  pool: Agent,
  url: string,
  extractPath: string,
  expectedIntegrity: string
): Promise<{ integrity: string; size: number }> {
  const response = await pool.fetch(url, { 
    dispatchOptions: { handlers: [/* streaming handler */] }
  });
  
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  
  const hash = createHash('sha512');
  let size = 0;
  
  await pipeline(
    Readable.fromWeb(response.body as any),
    new TransformStream({
      transform(chunk, controller) {
        hash.update(chunk);
        size += chunk.length;
        controller.enqueue(chunk);
      }
    }),
    createExtract(extractPath, { strip: 1 })
  );
  
  const actualIntegrity = `sha512-${hash.digest('base64')}`;
  if (actualIntegrity !== expectedIntegrity) {
    throw new Error(`Integrity mismatch: expected ${expectedIntegrity}, got ${actualIntegrity}`);
  }
  return { integrity: actualIntegrity, size };
}
```

### 3.3 src/fetcher/index.ts - High-level API
```typescript
export class Fetcher {
  constructor(
    private store: StoreBackend,
    private pool: Agent,
    private registryUrl: string
  ) {}

  async fetchMultiple(packages: ResolvedDependency[]): Promise<FetchResult[]>;
  async fetchSingle(pkg: ResolvedDependency): Promise<FetchResult>;
  
  // Offline mode support
  async fetchFromStore(pkg: ResolvedDependency): Promise<FetchResult | null>;
}
```

### 3.4 Tasks
- [ ] Add `undici`, `tar-fs` dependencies
- [ ] Write fetcher/pool.ts
- [ ] Write fetcher/streamExtract.ts
- [ ] Write fetcher/index.ts (replace current fetcher.ts)
- [ ] Unit tests: streaming extract, integrity verify, pool reuse

## Phase 1: Resolver - Conflict Detection (Week 2-3)

### 4.1 src/resolver/graph.ts - Dependency Graph
```typescript
export interface DependencyNode {
  name: string;
  range: string;
  resolved?: ResolvedDependency;
  dependencies: Map<string, DependencyNode>;  // name -> node
  dependents: Set<string>;  // packages that depend on this
}

export class DependencyGraph {
  private nodes = new Map<string, DependencyNode>();  // name@version
  
  addNode(name: string, range: string): DependencyNode;
  getNode(name: string, version: string): DependencyNode | undefined;
  detectConflicts(): Conflict[];  // Same name, different versions
  getResolutionOrder(): DependencyNode[];  // Topological sort
}

export interface Conflict {
  name: string;
  versions: Array<{ version: string; requiredBy: string[] }>;
}
```

### 4.2 src/resolver/conflict.ts - Version Resolution
```typescript
export function resolveConflicts(
  conflicts: Conflict[],
  registry: RegistryClient
): Promise<ResolutionDecision[]>;

// Strategy:
// 1. If versions compatible (semver intersects) -> hoist highest compatible
// 2. If not compatible -> duplicate in node_modules (last resort)
// 3. Prefer locked versions from lockfile
// 4. Report unresolvable conflicts as errors
```

### 4.3 src/resolver/index.ts - Main Resolver
```typescript
export class Resolver {
  constructor(
    private registry: RegistryClient,
    private store: StoreBackend,
    private security: SecurityManager,
    private config: MegagateConfig
  ) {}

  async resolve(
    manifests: Map<string, PackageManifest>,  // workspace packages
    lockfile: LockfileV1 | null,
    options: InstallOptions
  ): Promise<ResolutionResult>;

  private async resolveWorkspacePackages(
    manifests: Map<string, PackageManifest>
  ): Promise<Map<string, ResolvedDependency>>;  // workspace:* protocol
}
```

### 4.4 src/resolver/peerValidation.ts
```typescript
export function validatePeerDependencies(
  graph: DependencyGraph,
  lockfile: LockfileV1
): PeerValidationResult {
  // Check each package's peerDependencies against actual resolved versions
  // Report: missing, incompatible, unmet optional
  // Config: warn | error | ignore
}
```

### 4.5 Tasks
- [ ] Write resolver/graph.ts
- [ ] Write resolver/conflict.ts
- [ ] Write resolver/peerValidation.ts
- [ ] Write resolver/index.ts (replace current resolver.ts)
- [ ] Unit tests: conflict detection, hoisting, peer validation

## Phase 1: Linker & Lockfile (Week 3)

### 5.1 src/linker/index.ts - Linking Strategies
```typescript
export interface LinkStrategy {
  link(pkg: PackageRef, target: string): Promise<void>;
  unlink(target: string): Promise<void>;
}

export class HardlinkStrategy implements LinkStrategy { ... }
export class SymlinkStrategy implements LinkStrategy { ... }
export class CopyStrategy implements LinkStrategy { ... }  // Windows fallback

export class Linker {
  constructor(
    private store: StoreBackend,
    private strategy: LinkStrategy,
    private config: MegagateConfig
  ) {}

  async link(importerPath: string, lockfile: LockfileV1): Promise<void>;
  async unlinkPackage(importerPath: string, pkgName: string): Promise<void>;
  async clean(importerPath: string): Promise<void>;
  
  // Virtual store: node_modules/.megagate/name@version -> store
  // Node modules: node_modules/name -> .megagate/name@version
  // Transitive: store/nodes/name/version/node_modules/dep -> ../../.megagate/dep@version
}
```

### 5.2 src/lockfile/index.ts - Lockfile Operations
```typescript
export class LockfileManager {
  constructor(private storeDir: string) {}

  load(cwd: string): Promise<LockfileV1 | null>;
  save(lockfile: LockfileV1, cwd: string): Promise<void>;
  createEmpty(): LockfileV1;
  
  // Content hash for determinism
  computeContentHash(lockfile: LockfileV1): string;
  
  // Migration
  migrateV1toV2(v1: LockfileV1): LockfileV2;
  
  // Verify
  verifyIntegrity(lockfile: LockfileV1): { valid: boolean; errors: string[] };
  
  // Export
  export(lockfile: LockfileV1, format: 'json' | 'yaml'): string;
}
```

### 5.3 Tasks
- [ ] Write linker/index.ts (strategies + main linker)
- [ ] Write lockfile/index.ts (replace current lock.ts)
- [ ] Unit tests: link strategies, lockfile save/load, contentHash

## Phase 1: Installer Orchestration (Week 3)

### 6.1 src/installer/index.ts
```typescript
export interface InstallResult {
  lock: LockfileV1;
  fetched: Map<string, FetchResult>;
  added: string[];
  updated: string[];
  removed: string[];
}

export class Installer {
  constructor(
    private resolver: Resolver,
    private fetcher: Fetcher,
    private linker: Linker,
    private lockfile: LockfileManager,
    private store: StoreBackend,
    private security: SecurityManager,
    private config: MegagateConfig
  ) {}

  async install(options: InstallOptions): Promise<InstallResult>;
  async add(spec: string, options: AddOptions): Promise<InstallResult>;
  async update(spec?: string, options: UpdateOptions): Promise<InstallResult>;
  async remove(name: string): Promise<InstallResult>;
  async list(depth: number): Promise<Record<string, string>>;
  async verify(): Promise<{ valid: boolean; errors: string[] }>;

  private async loadPackageJson(cwd: string): Promise<PackageManifest>;
  private async writePackageJson(cwd: string, manifest: PackageManifest): Promise<void>;
}
```

### 6.2 Tasks
- [ ] Write installer/index.ts (replace current installer.ts)
- [ ] Integration tests: full install/add/update/remove cycle

---

## Phase 1: Workspace / Monorepo (Week 3-4)

### 7.1 src/workspace/config.ts
```typescript
// megagate.workspace.json
export interface WorkspaceConfig {
  packages: string[];  // globs: "packages/*", "apps/*"
  catalog?: Record<string, string>;  // "react": "^18.2.0"
  overrides?: Record<string, string>;  // force versions
  linkWorkspacePackages?: 'shallow' | 'deep' | false;
}

export function loadWorkspaceConfig(root: string): Promise<WorkspaceConfig | null>;
export function discoverWorkspacePackages(config: WorkspaceConfig): Promise<WorkspacePackage[]>;

export interface WorkspacePackage {
  path: string;
  manifest: PackageManifest;
  relativePath: string;
}
```

### 7.2 src/workspace/protocol.ts - workspace:* Resolver
```typescript
export function resolveWorkspaceProtocol(
  spec: string,  // "workspace:*" or "workspace:^1.0.0"
  workspacePackages: WorkspacePackage[]
): ResolvedDependency | null;
```

### 7.3 src/workspace/catalog.ts - Version Catalog
```typescript
export function resolveCatalogSpec(
  spec: string,  // "catalog:react"
  catalog: Record<string, string>
): string | null;
```

### 7.4 src/workspace/filter.ts - --filter Selector
```typescript
export interface FilterSelector {
  type: 'package' | 'dir' | 'glob' | 'since' | 'dependents' | 'dependencies';
  value: string;
}

export function parseFilterSelector(input: string): FilterSelector;
export function matchPackages(selector: FilterSelector, packages: WorkspacePackage[]): WorkspacePackage[];
```

### 7.5 Tasks
- [ ] Write workspace/config.ts
- [ ] Write workspace/protocol.ts
- [ ] Write workspace/catalog.ts
- [ ] Write workspace/filter.ts
- [ ] Integrate into resolver + installer + CLI
- [ ] CLI: `megagate install --filter=...`, `megagate -r <cmd>`
- [ ] Integration tests: monorepo with 5+ packages

## Phase 1: CLI Commands (Week 4)

### 8.1 src/cli/commands/install.ts
```typescript
export function createInstallCommand(installer: Installer): Command {
  return new Command('install')
    .description('Install dependencies')
    .option('--frozen-lockfile', 'Fail if lockfile out of sync')
    .option('--production', 'Skip devDependencies')
    .option('--offline', 'Offline mode')
    .option('--prefer-offline', 'Prefer cached')
    .option('--ignore-minimum-age', 'Skip minimumReleaseAge check')
    .action(async (opts) => { ... });
}
```

### 8.2 src/cli/commands/security.ts
```typescript
export function createSecurityCommand(installer: Installer): Command {
  const cmd = new Command('security')
    .description('Security commands');
  
  cmd.command('approve-builds <pkg>')
    .option('-s, --script <name>', 'Script to approve')
    .description('Approve lifecycle script for package');
  
  cmd.command('audit')
    .option('--format <json|text>', 'Output format')
    .description('Audit dependencies for vulnerabilities');
  
  return cmd;
}
```

### 8.3 src/cli/commands/workspace.ts
```typescript
export function createWorkspaceCommand(installer: Installer): Command {
  const cmd = new Command('workspace')
    .description('Workspace commands');
  
  cmd.command('run <script>')
    .option('-r, --recursive', 'Run in all packages')
    .option('--filter <selector>', 'Filter packages')
    .description('Run script in workspace packages');
  
  return cmd;
}
```

### 8.4 src/cli/index.ts - Main Entry
```typescript
export function createCLI(installer: Installer): Command {
  const program = new Command()
    .name('megagate')
    .description('MegaGate Package Manager')
    .version(VERSION);
  
  program.addCommand(createInstallCommand(installer));
  program.addCommand(createAddCommand(installer));
  program.addCommand(createUpdateCommand(installer));
  program.addCommand(createRemoveCommand(installer));
  program.addCommand(createListCommand(installer));
  program.addCommand(createVerifyCommand(installer));
  program.addCommand(createStoreCommand(installer));
  program.addCommand(createSecurityCommand(installer));
  program.addCommand(createWorkspaceCommand(installer));
  // Phase 2:
  // program.addCommand(createDevCommand(installer));
  // program.addCommand(createBuildCommand(installer));
  // program.addCommand(createTestCommand(installer));
  // program.addCommand(createExecCommand(installer));
  // program.addCommand(createInitCommand(installer));
  
  return program;
}
```

### 8.5 Tasks
- [ ] Create cli/commands/ folder with individual command files
- [ ] Write cli/index.ts
- [ ] Update package.json bin entry
- [ ] Test all CLI commands

## Phase 2: Runtime - Dev Toolchain (Week 5-8)

### 9.1 src/runtime/tsExecutor.ts - TypeScript Executor
```typescript
// Zero-config TS execution using oxc (fast) or TypeScript Compiler API
export interface TSExecutorOptions {
  cacheDir: string;           // ~/.megagate/ts-cache
  target: string;             // ES2022
  module: 'esnext' | 'commonjs';
  jsx: 'react' | 'react-jsx' | 'preserve';
  strict: boolean;
}

export class TSExecutor {
  constructor(private options: TSExecutorOptions) {}

  async execute(filePath: string, args: string[]): Promise<ExecuteResult> {
    // 1. Check cache (content-hash based)
    // 2. Transform: strip types, JSX -> JS
    // 3. Write to cache
    // 4. Execute with Node.js (--import cache loader)
  }

  async transform(code: string, filePath: string): Promise<TransformResult> {
    // Use oxc for speed, fallback to TypeScript API
    // Pure type-stripping, preserve runtime behavior
  }

  // Hot reload support
  async watch(filePath: string, onChange: () => void): Promise<void>;
}
```

### 9.2 src/runtime/devServer.ts - Dev Server + HMR
```typescript
export interface DevServerConfig {
  port: number;
  host: string;
  entryPoints: string[];
  hmr: boolean;
  https?: { key: string; cert: string };
  proxy?: Record<string, string>;
}

export class DevServer {
  constructor(
    private tsExecutor: TSExecutor,
    private bundler: Bundler,
    private config: DevServerConfig
  ) {}

  async start(): Promise<ServerHandle> {
    // - File watcher (chokidar)
    // - HMR: WebSocket broadcast on file change
    // - Transform on-demand (no full bundle)
    // - HTML entry point support
    // - Proxy API for backend
  }
}
```

### 9.3 src/runtime/bundler.ts - Production Bundler
```typescript
// Wrapper around esbuild (Go) or rolldown (Rust)
export interface BuildOptions {
  entryPoints: string[];
  outDir: string;
  format: 'esm' | 'cjs' | 'iife';
  target: string;
  splitting: boolean;
  sourcemap: boolean;
  minify: boolean;
  external?: string[];
}

export class Bundler {
  async build(options: BuildOptions): Promise<BuildResult>;
  async watch(options: BuildOptions, onRebuild: () => void): Promise<void>;
}
```

### 9.4 src/runtime/testRunner.ts - Test Runner
```typescript
export interface TestConfig {
  patterns: string[];           // ['**/*.test.ts', '**/*.spec.ts']
  coverage: boolean;
  reporters: string[];          // ['default', 'junit']
  timeout: number;
  parallel: boolean;
}

export class TestRunner {
  constructor(
    private tsExecutor: TSExecutor,
    private config: TestConfig
  ) {}

  async run(patterns?: string[]): Promise<TestResult> {
    // - Discover test files
    // - Run in isolated contexts (vm or worker_threads)
    // - Snapshot testing, mocking API
    // - Coverage via v8 (Node.js --experimental-vm-modules)
    // - Parallel execution, watch mode
    // - Compatible API with Vitest/Jest
  }
}
```

### 9.5 src/runtime/templates/ - Project Templates
```
templates/
├── vanilla/          # HTML + TS + CSS (lockdownMode: true)
│   ├── index.html
│   ├── main.ts
│   ├── style.css
│   └── megagate.toml
├── react/
├── vue/
├── svelte/
├── solid/
├── node-library/     # Library mode
└── worker/           # Cloudflare Workers, etc.
```

### 9.6 Tasks
- [ ] Add dependencies: `oxc`, `esbuild`, `chokidar`, `vitest` (core only)
- [ ] Write runtime/tsExecutor.ts
- [ ] Write runtime/devServer.ts
- [ ] Write runtime/bundler.ts
- [ ] Write runtime/testRunner.ts
- [ ] Create templates/
- [ ] CLI commands: dev, build, test, exec, init
- [ ] Integration tests: create project, dev, build, test

## Phase 3: Production Hardening (Week 9-12)

### 10.1 src/cli/commands/migrate.ts - Migration Tools
```typescript
export async function migrateFromPnpm(lockPath: string): Promise<LockfileV1>;
export async function migrateFromNpm(lockPath: string): Promise<LockfileV1>;
export async function migrateFromYarn(lockPath: string): Promise<LockfileV1>;
export async function migrateFromBun(lockPath: string): Promise<LockfileV1>;

CLI:
megagate migrate from-pnpm [--input pnpm-lock.yaml] [--output megagate-lock.json]
megagate migrate from-npm [--input package-lock.json]
megagate migrate from-yarn [--input yarn.lock]
megagate migrate from-bun [--input bun.lockb]
```

### 10.2 src/cli/commands/doctor.ts
```typescript
export async function runDoctor(): Promise<DoctorReport> {
  // - Check Node.js version compatibility
  // - Check store integrity
  // - Check lockfile consistency
  // - Check peer dependency issues
  // - Check disk space
  // - Check network connectivity to registry
  // - Check config validity
}
```

### 10.3 src/cli/commands/audit.ts
```typescript
export async function runAudit(lockfile: LockfileV1): Promise<AuditReport> {
  // - Query OSV / GitHub Advisory Database
  // - Match vulnerabilities against locked packages
  // - Report: severity, CVE, fixed version, recommendation
  // - Exit code: 0=none, 1=low, 2=moderate, 3=high, 4=critical
}
```

### 10.4 src/cli/commands/why.ts
```typescript
export async function runWhy(lockfile: LockfileV1, pkgName: string): Promise<WhyResult> {
  // - Reverse dependency graph
  // - Show: direct deps, transitive deps, workspace deps
  // - Explain why version was selected
}
```

### 10.5 Single Binary (Node.js SEA)
```typescript
// scripts/buildSea.ts
import { build } from 'esbuild';
import { copyFileSync } from 'fs';
import { resolve } from 'path';
import { Sea } from 'node:sea';

await build({
  entryPoints: ['src/cli/index.ts'],
  bundle: true,
  platform: 'node',
  target: 'node20',
  outfile: 'dist/sea-prep.js',
  format: 'cjs',
});

const sea = new Sea({ main: 'dist/sea-prep.js', output: 'megagate' });
await sea.link();
```

### 10.6 Rust Acceleration (napi-rs) - Phase 3+
```rust
// packages/core-native/src/lib.rs
#[napi]
pub fn compute_integrity(data: &[u8]) -> String { ... }

#[napi]
pub fn resolve_versions(deps: Vec<DependencySpec>) -> Vec<ResolvedVersion> { ... }

#[napi]
pub fn link_package(store_path: &str, target_path: &str, strategy: i32) -> Result<()> { ... }
```

### 10.7 Tasks
- [ ] Write migrate commands
- [ ] Write doctor command
- [ ] Write audit command (OSV integration)
- [ ] Write why command
- [ ] Write outdated command
- [ ] Build single binary (SEA)
- [ ] Setup napi-rs project structure
- [ ] Benchmark before/after Rust acceleration

## Phase 4: Ecosystem & Polish (Week 13-14)

### 11.1 Provenance & SLSA (src/security/provenance.ts)
```typescript
export class ProvenanceVerifier {
  async verify(pkg: LockedPackage): Promise<VerificationResult>;
  async generateAttestation(pkgPath: string): Promise<Attestation>;
}

// SLSA Level 1: Provenance generation
// SLSA Level 2: Tamper-resistant build
// SLSA Level 3: Hardened build
```

### 11.2 SBOM & License Compliance (src/security/sbom.ts)
```typescript
export async function generateSBOM(lockfile: LockfileV1): Promise<SBOM> {
  // SPDX 2.3 or CycloneDX 1.5
  // Include: name, version, license, copyright, homepage
}

export async function checkLicenses(
  lockfile: LockfileV1,
  allowed: string[]  // ['MIT', 'Apache-2.0', 'BSD-3-Clause']
): Promise<LicenseReport>;
```

### 11.3 Plugin System (src/core/plugins.ts)
```typescript
interface MegagatePlugin {
  name: string;
  hooks: {
    'pre-install'?: (ctx: InstallContext) => Promise<void>;
    'post-install'?: (ctx: InstallContext) => Promise<void>;
    'pre-fetch'?: (pkg: ResolvedDependency) => Promise<ResolvedDependency>;
    'transform'?: (code: string, id: string) => Promise<string>;
    'resolve'?: (spec: string) => Promise<string | null>;
  };
}

export class PluginManager {
  load(plugins: string[]): Promise<void>;
  executeHook(hook: string, ctx: any): Promise<void>;
}
```

### 11.4 Documentation & Website
- docs/ (VitePress): Getting started, Config, CLI, Migration, Architecture
- Interactive playground (StackBlitz-style)
- Auto-generated CLI reference

### 11.5 Benchmarks (benchmarks/run.ts)
```typescript
export async function runBenchmarks(): Promise<BenchmarkReport> {
  // Cold install (100 packages)
  // Warm install (cache hit)
  // Monorepo (20 packages, 500 deps)
  // Dev server startup
  // Build time (10k modules)
  // Test runner (1k tests)
  // Memory usage
}
```

### 11.6 Tasks
- [ ] Provenance/SLSA support
- [ ] SBOM generation
- [ ] License compliance checker
- [ ] Plugin system
- [ ] Documentation website
- [ ] Benchmark suite with CI regression detection
