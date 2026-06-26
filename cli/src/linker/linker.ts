import { mkdir, writeFile, readFile, symlink, unlink, rm, access, constants, readdir, lstat } from 'fs/promises';
import { join, dirname, relative, resolve } from 'path';
import { LockFile, LockPackage } from '../types.js';
import { getStoreDir, getPackageStorePath, createVirtualStoreLink, createPackageLink, createNodeModulesLink, ensureStoreDirs } from '../store.js';

export interface LinkOptions {
  cwd?: string;
  storeDir?: string;
  production?: boolean;
}

export class Linker {
  private cwd: string;
  private storeDir: string;
  private production: boolean;
  private nodeModulesDir: string;
  private virtualStoreDir: string;

  constructor(options: LinkOptions = {}) {
    this.cwd = resolve(options.cwd || process.cwd());
    this.storeDir = getStoreDir(options.storeDir);
    this.production = options.production || false;
    this.nodeModulesDir = join(this.cwd, 'node_modules');
    this.virtualStoreDir = createVirtualStoreLink(this.nodeModulesDir);
  }

  async link(lock: LockFile): Promise<void> {
    await this.prepareDirectories();

    const importer = lock.importers['.'] || { dependencies: {}, devDependencies: {}, optionalDependencies: {} };
    const allDeps = {
      ...importer.dependencies,
      ...(this.production ? {} : importer.devDependencies),
      ...importer.optionalDependencies,
    };

    for (const [name, version] of Object.entries(allDeps)) {
      await this.linkPackage(name, version, lock);
    }

    await this.linkTransitiveDeps(lock);
  }

  private async prepareDirectories(): Promise<void> {
    await mkdir(this.nodeModulesDir, { recursive: true });
    await mkdir(this.virtualStoreDir, { recursive: true });
  }

  private async linkPackage(name: string, version: string, lock: LockFile): Promise<void> {
    const pkgKey = `${name}@${version}`;
    const pkg = lock.packages[pkgKey];
    if (!pkg) {
      throw new Error(`Package not found in lock: ${pkgKey}`);
    }

    const storePkgPath = getPackageStorePath(this.storeDir, name, version);
    const virtualPkgLink = createPackageLink(this.virtualStoreDir, name, version);
    const nodeModulesLink = createNodeModulesLink(this.nodeModulesDir, name);

    await this.ensureSymlink(storePkgPath, virtualPkgLink);
    await this.ensureSymlink(virtualPkgLink, nodeModulesLink);

    await this.linkPackageDeps(name, version, lock);
  }

  private async linkPackageDeps(name: string, version: string, lock: LockFile): Promise<void> {
    const pkgKey = `${name}@${version}`;
    const pkg = lock.packages[pkgKey];
    if (!pkg?.dependencies) return;

    const pkgNodeModules = join(getPackageStorePath(this.storeDir, name, version), 'node_modules');
    await mkdir(pkgNodeModules, { recursive: true });

    for (const [depName, depVersion] of Object.entries(pkg.dependencies)) {
      const depStorePath = getPackageStorePath(this.storeDir, depName, depVersion);
      const depLink = join(pkgNodeModules, depName);
      await this.ensureSymlink(depStorePath, depLink);
    }
  }

  private async linkTransitiveDeps(lock: LockFile): Promise<void> {
    for (const [pkgKey, pkg] of Object.entries(lock.packages)) {
      if (!pkg.dependencies) continue;

      const pkgNodeModules = join(getPackageStorePath(this.storeDir, pkg.name, pkg.version), 'node_modules');
      await mkdir(pkgNodeModules, { recursive: true });

      for (const [depName, depVersion] of Object.entries(pkg.dependencies)) {
        const depStorePath = getPackageStorePath(this.storeDir, depName, depVersion);
        const depLink = join(pkgNodeModules, depName);
        await this.ensureSymlink(depStorePath, depLink);
      }
    }
  }

  private async ensureSymlink(target: string, linkPath: string): Promise<void> {
    try {
      await access(linkPath, constants.F_OK);
      const existingTarget = await readFile(linkPath, 'utf-8').catch(() => null);
      if (existingTarget === target) return;
      await unlink(linkPath);
    } catch {
    }

    await mkdir(dirname(linkPath), { recursive: true });
    await symlink(target, linkPath, 'dir');
  }

  async unlinkPackage(name: string): Promise<void> {
    const nodeModulesLink = createNodeModulesLink(this.nodeModulesDir, name);
    const virtualStoreLink = createVirtualStoreLink(this.nodeModulesDir);

    try {
      await unlink(nodeModulesLink);
    } catch {
    }

    const entries = await readdir(virtualStoreLink).catch(() => []);
    let hasOtherVersions = false;
    for (const entry of entries) {
      if (entry.startsWith(`${name}@`)) {
        hasOtherVersions = true;
        break;
      }
    }

    if (!hasOtherVersions) {
      try {
        await rm(virtualStoreLink, { recursive: true, force: true });
      } catch {
      }
    }
  }

  async clean(): Promise<void> {
    await rm(this.nodeModulesDir, { recursive: true, force: true });
  }
}

export function createLinker(options?: LinkOptions): Linker {
  return new Linker(options);
}