import semver from 'semver';
import { RegistryClient, createRegistryClient } from './registry.js';
import { LockFile, LockPackage, ResolvedDependency, PackageJson, InstallOptions } from './types.js';
import { lockKey, addPackageToLock, createEmptyLock, updateImporter, getPackageFromLock } from './lock.js';
import { getStoreDir, packageExistsInStore } from './store.js';

export interface ResolutionResult {
    lock: LockFile;
    newPackages: Map<string, ResolvedDependency>;
}

export class Resolver {
    private registry: RegistryClient;
    private storeDir: string;
    private lock: LockFile;
    private resolvedCache = new Map<string, ResolvedDependency>();

    constructor(lock: LockFile, options: InstallOptions = {}) {
        this.registry = createRegistryClient({ registry: options.registry });
        this.storeDir = getStoreDir(options.storeDir);
        this.lock = lock;
    }

    async resolveAll(
        pkgJson: PackageJson,
        options: InstallOptions = {}
    ): Promise<ResolutionResult> {
        const deps = pkgJson.dependencies || {};
        const devDeps = options.production ? {} : pkgJson.devDependencies || {};
        const optionalDeps = pkgJson.optionalDependencies || {};

        const newPackages = new Map<string, ResolvedDependency>();
        const resolvedDeps: Record<string, string> = {};
        const resolvedDevDeps: Record<string, string> = {};
        const resolvedOptionalDeps: Record<string, string> = {};

        await this.resolveDependencies(deps, devDeps, optionalDeps, newPackages, resolvedDeps, resolvedDevDeps, resolvedOptionalDeps);

        updateImporter(this.lock, '.', resolvedDeps, resolvedDevDeps, resolvedOptionalDeps);

        return { lock: this.lock, newPackages };
    }

    private async resolveDependencies(
        deps: Record<string, string>,
        devDeps: Record<string, string>,
        optionalDeps: Record<string, string>,
        newPackages: Map<string, ResolvedDependency>,
        resolvedDeps: Record<string, string>,
        resolvedDevDeps: Record<string, string>,
        resolvedOptionalDeps: Record<string, string>
    ): Promise<void> {
        const allDeps = { ...deps, ...devDeps, ...optionalDeps };

        for (const [name, range] of Object.entries(allDeps)) {
            const resolved = await this.resolveSingle(name, range, newPackages);
            if (deps[name]) resolvedDeps[name] = resolved.version;
            else if (devDeps[name]) resolvedDevDeps[name] = resolved.version;
            else if (optionalDeps[name]) resolvedOptionalDeps[name] = resolved.version;
        }
    }

    private async resolveSingle(
        name: string,
        range: string,
        newPackages: Map<string, ResolvedDependency>
    ): Promise<ResolvedDependency> {
        const cacheKey = `${name}@${range}`;
        if (this.resolvedCache.has(cacheKey)) {
            return this.resolvedCache.get(cacheKey)!;
        }

        const resolved = await this.registry.resolveDependency(name, range);
        const lockKeyResolved = lockKey(resolved.name, resolved.version);

        const existing = getPackageFromLock(this.lock, resolved.name, resolved.version);
        if (existing) {
            const fromLock: ResolvedDependency = {
                name: existing.name,
                version: existing.version,
                integrity: existing.integrity,
                resolved: existing.resolved,
                size: existing.size,
                dependencies: existing.dependencies,
                optionalDependencies: existing.optionalDependencies,
                peerDependencies: existing.peerDependencies,
                bin: existing.bin,
                engines: existing.engines,
            };
            this.resolvedCache.set(cacheKey, fromLock);
            return fromLock;
        }

        this.resolvedCache.set(cacheKey, resolved);

        const lockPkg: LockPackage = {
            name: resolved.name,
            version: resolved.version,
            integrity: resolved.integrity,
            dependencies: resolved.dependencies,
            optionalDependencies: resolved.optionalDependencies,
            peerDependencies: resolved.peerDependencies,
            bin: resolved.bin,
            engines: resolved.engines,
            resolved: resolved.resolved,
            size: resolved.size,
        };
        addPackageToLock(this.lock, lockPkg);

        if (resolved.dependencies) {
            await this.resolveDependencies(
                resolved.dependencies,
                {},
                resolved.optionalDependencies || {},
                newPackages,
                {},
                {},
                {}
            );
        }

        newPackages.set(cacheKey, resolved);
        return resolved;
    }

    async resolveTransitive(
        dep: ResolvedDependency,
        newPackages: Map<string, ResolvedDependency>
    ): Promise<void> {
        if (!dep.dependencies) return;

        for (const [name, range] of Object.entries(dep.dependencies)) {
            await this.resolveSingle(name, range, newPackages);
        }
    }
}

export async function resolveDependencies(
    pkgJson: PackageJson,
    existingLock: LockFile | null,
    options: InstallOptions = {}
): Promise<ResolutionResult> {
    const lock = existingLock || createEmptyLock(getStoreDir(options.storeDir));
    const resolver = new Resolver(lock, options);
    return resolver.resolveAll(pkgJson, options);
}