import { createHash } from 'crypto';
import { mkdir, writeFile, readFile, access, constants, rm, symlink } from 'fs/promises';
import { join, dirname } from 'path';
import { ResolvedDependency, type FetchResult, LockFile } from './types.js';
import { getTarballPath, getIntegrityPath, getPackageStorePath, writeStoreMeta, computeIntegrity, verifyIntegrity, packageExistsInStore, ensureStoreDirs } from './store.js';
import { getPackageFromLock } from './lock.js';
import { RegistryClient, createRegistryClient } from './registry.js';
import { createReadStream } from 'fs';
import tar from 'tar';
import { Readable } from 'stream';

export class Fetcher {
  private storeDir: string;
  private registry: RegistryClient;
  private lock: LockFile | null = null;

  constructor(storeDir: string, registry?: string, lock?: LockFile) {
    this.storeDir = storeDir;
    this.registry = createRegistryClient({ registry });
    this.lock = lock || null;
  }

  setLock(lock: LockFile): void {
    this.lock = lock;
  }

  async fetchMultiple(packages: Map<string, ResolvedDependency>): Promise<Map<string, FetchResult>> {
    await ensureStoreDirs(this.storeDir);
    const results = new Map<string, FetchResult>();

    for (const [key, pkg] of packages) {
      const result = await this.fetchSingle(pkg);
      results.set(key, result);
    }

    return results;
  }

  async fetchSingle(pkg: ResolvedDependency): Promise<FetchResult> {
    const tarballPath = getTarballPath(this.storeDir, pkg.name, pkg.version);
    const integrityPath = getIntegrityPath(this.storeDir, pkg.name, pkg.version);
    const extractPath = getPackageStorePath(this.storeDir, pkg.name, pkg.version);

    if (await packageExistsInStore(this.storeDir, pkg.name, pkg.version)) {
      const verified = await verifyIntegrity(tarballPath, pkg.integrity);
      if (verified) {
        return {
          tarballPath,
          extractPath,
          integrity: pkg.integrity,
          size: pkg.size,
        };
      }
    }

    const tarball = await this.registry.downloadTarball(pkg.resolved);
    const actualIntegrity = computeIntegrity(tarball);

    if (actualIntegrity !== pkg.integrity) {
      throw new Error(
        `Integrity mismatch for ${pkg.name}@${pkg.version}: expected ${pkg.integrity}, got ${actualIntegrity}`
      );
    }

    await mkdir(dirname(tarballPath), { recursive: true });
    await writeFile(tarballPath, tarball);
    await writeFile(integrityPath, pkg.integrity, 'utf-8');

    await this.extractTarball(tarball, extractPath, pkg);

    await writeStoreMeta(this.storeDir, pkg.name, pkg.version, {
      integrity: pkg.integrity,
      size: tarball.length,
      extractedAt: new Date().toISOString(),
    });

    return {
      tarballPath,
      extractPath,
      integrity: pkg.integrity,
      size: tarball.length,
    };
  }

  private async extractTarball(
    tarball: Buffer,
    extractPath: string,
    pkg: ResolvedDependency
  ): Promise<void> {
    await mkdir(extractPath, { recursive: true });

    await new Promise<void>((resolve, reject) => {
      const readable = new Readable();
      readable.push(tarball);
      readable.push(null);

      readable
        .pipe(tar.x({ C: extractPath, strip: 1 }))
        .on('close', resolve)
        .on('error', reject);
    });

    const pkgJsonPath = join(extractPath, 'package.json');
    try {
      const pkgContent = await readFile(pkgJsonPath, 'utf-8');
      const pkgJson = JSON.parse(pkgContent);
      if (pkgJson.dependencies || pkgJson.devDependencies || pkgJson.optionalDependencies) {
        await this.linkDependencies(pkg.name, pkg.version, pkgJson, extractPath);
      }
    } catch {
      // No package.json or no deps
    }
  }

  private getResolvedVersion(depName: string, depRange: string): string {
    if (!this.lock) return depRange;

    // First try to find in lock by exact name@version
    // The lock uses name@resolvedVersion as key
    for (const [key, lockPkg] of Object.entries(this.lock.packages)) {
      if (lockPkg.name === depName) {
        return lockPkg.version;
      }
    }

    // Fallback to the range (will fail later if not in store)
    return depRange;
  }

  private async linkDependencies(
    name: string,
    version: string,
    pkgJson: any,
    extractPath: string
  ): Promise<void> {
    const allDeps = {
      ...pkgJson.dependencies,
      ...pkgJson.devDependencies,
      ...pkgJson.optionalDependencies,
    };

    const nodeModulesPath = join(extractPath, 'node_modules');
    await mkdir(nodeModulesPath, { recursive: true });

    for (const [depName, depRange] of Object.entries(allDeps as Record<string, string>)) {
      const resolvedVersion = this.getResolvedVersion(depName, depRange);
      const depPkgPath = getPackageStorePath(this.storeDir, depName, resolvedVersion);
      const linkPath = join(nodeModulesPath, depName);

      try {
        await access(depPkgPath, constants.R_OK);
        try {
          await access(linkPath, constants.F_OK);
          await rm(linkPath);
        } catch {
          // Link doesn't exist, that's fine
        }
        await symlink(depPkgPath, linkPath, 'dir');
      } catch {
        // Dependency not in store yet - will be linked later by main linker
      }
    }
  }
}

export function createFetcher(storeDir: string, registry?: string, lock?: LockFile): Fetcher {
  return new Fetcher(storeDir, registry, lock);
}